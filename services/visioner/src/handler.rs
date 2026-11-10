//! `visioner.v1.VisionerService/Check`, a server stream. The story of one check:
//! 1. take a screenshot from the sensor instance (`screen`: `capturing` → `captured`)
//! 2. ask every model at once, passing each one's progress on (`<id>`: `started` … `done|failed|timeout`)
//! 3. return every answer; the check fails only with no screenshot or when no model answered
//!
//! Everything runs inside the response stream: when the client goes away, every call still running is dropped.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::future::Future;
use std::time::{Duration, Instant};

use connectrpc::{ConnectError, ErrorCode, RequestContext, Response, ServiceRequest, ServiceResult, ServiceStream};
use contracts::proto::llm_caller::v1::{
    CallRequest, CallResult, ChatMessage, Image, LlmCallerServiceClient, call_response,
};
use contracts::proto::sensor::v1::{CaptureRequest, CaptureResponse, SensorServiceClient};
use contracts::proto::visioner::v1::{
    CheckProgress, CheckRequest, CheckResponse, CheckResult, LlmAnswer, VisionerService, check_response::Event,
};
use futures::future::join_all;
use kernel::{Channel, Deps, Logger, Metrics, StreamTx, kv, stream_from};

use crate::config::VisionerSettings;

/// The question asked when the request has none.
const DEFAULT_QUESTION: &str = "What do you see on the screen?";

/// The JSON every model must answer with.
const ANSWER_SCHEMA: &str = r#"{"type":"object","properties":{"summary":{"type":"string"},"details":{"type":"array","items":{"type":"string"}}},"required":["summary","details"]}"#;

// ---------------------------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------------------------

/// Holds a generated client per instance it calls. Cheap to clone (the clients share their channel).
#[derive(Clone)]
pub struct VisionerHandler {
    screen: SensorServiceClient<Channel>,
    llms: Vec<Llm>,
    screen_timeout: Duration,
    llm_timeout: Duration,
    log: Logger,
    metrics: Metrics,
}

/// One llm_caller instance.
#[derive(Clone)]
struct Llm {
    id: String,
    client: LlmCallerServiceClient<Channel>,
}

impl VisionerHandler {
    /// Every call carries its timeout as a deadline, so the called instance stops too when it passes.
    pub fn new(deps: &Deps, settings: &VisionerSettings) -> Self {
        let screen_config = deps.client_config().with_default_timeout(settings.screen_timeout);
        let llm_config = deps.client_config().with_default_timeout(settings.llm_timeout);
        let llms = settings
            .llms
            .iter()
            .map(|id| Llm {
                id: id.clone(),
                client: LlmCallerServiceClient::new(deps.channel_to(id), llm_config.clone()),
            })
            .collect();
        Self {
            screen: SensorServiceClient::new(deps.channel_to(&settings.screen), screen_config),
            llms,
            screen_timeout: settings.screen_timeout,
            llm_timeout: settings.llm_timeout,
            log: deps.logger("visioner").with([kv("visioner_id", settings.id.as_str())]),
            metrics: deps.metrics("visioner"),
        }
    }
}

#[allow(refining_impl_trait)]
impl VisionerService for VisionerHandler {
    async fn check(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, CheckRequest>,
    ) -> ServiceResult<ServiceStream<CheckResponse>> {
        let question = match request.question.trim() {
            "" => DEFAULT_QUESTION.to_string(),
            question => question.to_string(),
        };
        let handler = self.clone();
        Response::ok(stream_from(move |tx| handler.run(question, tx)))
    }
}

// ---------------------------------------------------------------------------------------------
// One check
// ---------------------------------------------------------------------------------------------

impl VisionerHandler {
    async fn run(self, question: String, tx: StreamTx<CheckResponse>) {
        let report = Reporter { tx: tx.clone(), started: Instant::now() };
        let screenshot = match self.capture(&report).await {
            Ok(screenshot) => screenshot,
            Err(error) => return tx.fail(error),
        };
        let request = ask_request(&question, &screenshot);
        let asks = self.llms.iter().map(|llm| self.ask(llm, request.clone(), report.clone()));
        let answers = join_all(asks).await; // ≈ `await Task.WhenAll(asks)`
        self.record_failures(&answers);
        if let Some(error) = all_failed(&answers) {
            return tx.fail(error);
        }
        let duration_ms = report.started.elapsed().as_millis() as u64;
        let result = CheckResult { screen: screenshot.metadata, answers, duration_ms, ..Default::default() };
        tx.send(CheckResponse { event: Some(Event::from(result)), ..Default::default() });
    }

    async fn capture(&self, report: &Reporter) -> Result<CaptureResponse, ConnectError> {
        report.send("screen", "capturing", "");
        let capture = self.screen.capture(CaptureRequest::default());
        let response = within(self.screen_timeout, "screen", async { Ok(capture.await?.into_owned()) })
            .await
            .map_err(|error| no_screenshot(&error))?;
        report.send("screen", "captured", "");
        Ok(response)
    }

    /// One model, within `llm_timeout`. On timeout its call is dropped, which cancels it.
    async fn ask(&self, llm: &Llm, request: CallRequest, report: Reporter) -> LlmAnswer {
        let started = Instant::now();
        let outcome = within(self.llm_timeout, &llm.id, call_llm(llm, request, &report)).await;
        let mut answer = LlmAnswer { caller_id: llm.id.clone(), ..Default::default() };
        match outcome {
            Ok(result) => {
                report.send(&llm.id, "done", &result.text);
                (answer.ok, answer.json, answer.model) = (true, result.json, result.metadata.model.clone());
            }
            Err(error) => {
                let stage = if error.code == ErrorCode::DeadlineExceeded { "timeout" } else { "failed" };
                answer.error_code = error.code.as_str().to_string();
                answer.error_message = error.message.unwrap_or_default();
                report.send(&llm.id, stage, &answer.error_message);
            }
        }
        answer.duration_ms = started.elapsed().as_millis() as u64;
        answer
    }

    /// A model that failed while others answered: the client still gets a result, so say it here.
    fn record_failures(&self, answers: &[LlmAnswer]) {
        for answer in answers.iter().filter(|a| !a.ok) {
            let (id, reason, error) =
                (answer.caller_id.as_str(), answer.error_code.as_str(), answer.error_message.as_str());
            self.log.warn("llm failed", [kv("caller_id", id), kv("reason", reason), kv("error", error)]);
            self.metrics.count("llm_failures", 1, [kv("caller_id", id), kv("reason", reason)]);
        }
    }
}

/// Run one `Call`: pass each progress message on, return the result.
async fn call_llm(llm: &Llm, request: CallRequest, report: &Reporter) -> Result<CallResult, ConnectError> {
    let mut stream = llm.client.call(request).await?;
    while let Some(message) = stream.message().await? {
        match message.to_owned_message().event {
            Some(call_response::Event::Progress(p)) => report.send(&llm.id, &p.stage, &p.partial_text),
            Some(call_response::Event::Result(result)) => return Ok(*result),
            None => {}
        }
    }
    Err(ConnectError::internal("the stream ended without a result"))
}

// ---------------------------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------------------------

/// Sends progress for any source (≈ C# `IProgress<T>`); each model's ask gets a clone.
#[derive(Clone)]
struct Reporter {
    tx: StreamTx<CheckResponse>,
    started: Instant,
}

impl Reporter {
    fn send(&self, source: &str, stage: &str, partial_text: &str) {
        let progress = CheckProgress {
            source: source.into(),
            stage: stage.into(),
            partial_text: partial_text.into(),
            elapsed_ms: self.started.elapsed().as_millis() as u32,
            ..Default::default()
        };
        self.tx.send(CheckResponse { event: Some(Event::from(progress)), ..Default::default() });
    }
}

// ---------------------------------------------------------------------------------------------
// Rules (pure, unit tested)
// ---------------------------------------------------------------------------------------------

/// `work`, or `deadline_exceeded` when it takes longer than `limit` (dropping it).
async fn within<T>(
    limit: Duration,
    what: &str,
    work: impl Future<Output = Result<T, ConnectError>>,
) -> Result<T, ConnectError> {
    match tokio::time::timeout(limit, work).await {
        Ok(outcome) => outcome,
        Err(_) => Err(ConnectError::deadline_exceeded(format!("{what}: no answer within {limit:?}"))),
    }
}

/// A failed capture fails the check: `deadline_exceeded` stays, anything else becomes `unavailable`.
fn no_screenshot(error: &ConnectError) -> ConnectError {
    let message = format!("no screenshot: {}", error.message.as_deref().unwrap_or_default());
    match error.code {
        ErrorCode::DeadlineExceeded => ConnectError::deadline_exceeded(message),
        _ => ConnectError::unavailable(message),
    }
}

/// The question and the screenshot as one user message, asking for `ANSWER_SCHEMA` JSON.
fn ask_request(question: &str, screenshot: &CaptureResponse) -> CallRequest {
    let mime_type = screenshot.metadata.mime_type.clone();
    let image = Image { data: screenshot.image.to_vec(), mime_type, ..Default::default() };
    let message =
        ChatMessage { role: "user".into(), content: question.into(), images: vec![image], ..Default::default() };
    CallRequest { messages: vec![message], json_schema: ANSWER_SCHEMA.into(), ..Default::default() }
}

/// `None` when at least one model answered; otherwise the error for the client, listing each failure.
/// `deadline_exceeded` when every model ran out of time, else `unavailable`.
fn all_failed(answers: &[LlmAnswer]) -> Option<ConnectError> {
    if answers.iter().any(|a| a.ok) {
        return None;
    }
    let failures: Vec<String> =
        answers.iter().map(|a| format!("{}: {} ({})", a.caller_id, a.error_message, a.error_code)).collect();
    let message = format!("every model failed: {}", failures.join("; "));
    let all_timed_out = answers.iter().all(|a| a.error_code == ErrorCode::DeadlineExceeded.as_str());
    Some(match all_timed_out {
        true => ConnectError::deadline_exceeded(message),
        false => ConnectError::unavailable(message),
    })
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
