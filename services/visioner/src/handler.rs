//! Proto ⇄ domain for the server-streaming `Check` RPC, plus logs, metrics and error codes.
//!
//! The check runs **inside** the response stream, not in a background task: the platform polls the stream
//! inside this call's trace span, so the screen and llm calls join the same trace. When the client goes
//! away (or its deadline passes) the stream is dropped, and with it every call still running.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use connectrpc::{ConnectError, ErrorCode, RequestContext, Response, ServiceRequest, ServiceResult, ServiceStream};
use contracts::proto::visioner::v1::{
    CheckProgress, CheckRequest, CheckResponse, CheckResult, LlmAnswer, ScreenInfo, VisionerService,
    check_response::Event,
};
use futures::StreamExt;
use kernel::{Deps, Logger, Metrics, kv};
use tokio::sync::mpsc;

use crate::domain::{Answer, CheckOutcome, Progress, VisionError, Visioner};

type Item = Result<CheckResponse, ConnectError>;

// ---------------------------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------------------------

pub struct VisionerHandler {
    runner: Arc<CheckRunner>,
}

/// Everything one check needs; shared by all checks of this instance.
struct CheckRunner {
    /// `Err(message)` when the instance is not configured: every call answers `failed_precondition`.
    visioner: Result<Visioner, String>,
    log: Logger,
    metrics: Metrics,
}

impl VisionerHandler {
    pub fn new(deps: &Deps, id: &str, visioner: Result<Visioner, String>) -> Self {
        let log = deps.logger("visioner").with([kv("visioner_id", id)]);
        Self { runner: Arc::new(CheckRunner { visioner, log, metrics: deps.metrics("visioner") }) }
    }
}

#[allow(refining_impl_trait)]
impl VisionerService for VisionerHandler {
    async fn check(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, CheckRequest>,
    ) -> ServiceResult<ServiceStream<CheckResponse>> {
        let question = request.question.to_string();
        let (tx, rx) = mpsc::unbounded_channel::<Item>();
        let work = self.runner.clone().run(question, tx);
        // `items` ends when `work` drops `tx`; `driver` only polls `work` and yields nothing.
        let items = futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) });
        let driver = futures::stream::once(work).filter_map(|()| async { None::<Item> });
        Response::ok(Box::pin(futures::stream::select(items, driver)))
    }
}

// ---------------------------------------------------------------------------------------------
// One check
// ---------------------------------------------------------------------------------------------

impl CheckRunner {
    async fn run(self: Arc<Self>, question: String, tx: mpsc::UnboundedSender<Item>) {
        let visioner = match &self.visioner {
            Ok(visioner) => visioner,
            Err(message) => {
                let _ = tx.send(Err(to_connect_error(VisionError::NotConfigured(message.clone()))));
                return;
            }
        };
        let report = |progress: Progress| {
            let _ = tx.send(Ok(progress_message(progress)));
        };
        let item = match visioner.check(&question, &report).await {
            Ok(outcome) => {
                self.record_success(&outcome);
                Ok(result_message(outcome))
            }
            Err(error) => {
                self.record_failure(&error);
                Err(to_connect_error(error))
            }
        };
        let _ = tx.send(item);
    }

    fn record_success(&self, outcome: &CheckOutcome) {
        let ok = outcome.answers.iter().filter(|a| a.outcome.is_ok()).count();
        let ms = outcome.duration.as_millis() as u64;
        self.log.info(
            "check done",
            [kv("answers_ok", ok as u64), kv("answers", outcome.answers.len() as u64), kv("ms", ms)],
        );
        self.metrics.count("checks", 1, [kv("result", "ok")]);
        self.metrics.record("check_ms", ms as f64, []);
        for answer in &outcome.answers {
            if let Err(failure) = &answer.outcome {
                self.log.warn(
                    "llm failed",
                    [kv("caller_id", answer.caller_id.as_str()), kv("error", failure.message.as_str())],
                );
                let tags = [kv("caller_id", answer.caller_id.as_str()), kv("reason", failure.code.as_str())];
                self.metrics.count("llm_failures", 1, tags);
            }
        }
    }

    fn record_failure(&self, error: &VisionError) {
        self.log.warn("check failed", [kv("reason", reason(error)), kv("error", error.to_string())]);
        self.metrics.count("checks", 1, [kv("result", reason(error))]);
    }
}

// ---------------------------------------------------------------------------------------------
// Mapping (pure)
// ---------------------------------------------------------------------------------------------

fn progress_message(progress: Progress) -> CheckResponse {
    let message = CheckProgress {
        source: progress.source,
        stage: progress.stage,
        partial_text: progress.partial_text,
        elapsed_ms: progress.elapsed.as_millis() as u32,
        ..Default::default()
    };
    CheckResponse { event: Some(Event::from(message)), ..Default::default() }
}

fn result_message(outcome: CheckOutcome) -> CheckResponse {
    let screen = ScreenInfo {
        sensor_id: outcome.screen.sensor_id,
        width: outcome.screen.width,
        height: outcome.screen.height,
        mime_type: outcome.screen.mime_type,
        bytes: outcome.screen.bytes,
        capture_ms: outcome.screen.capture_ms,
        ..Default::default()
    };
    let answers = outcome.answers.into_iter().map(answer_message).collect();
    let duration_ms = outcome.duration.as_millis() as u64;
    let result = CheckResult { screen: screen.into(), answers, duration_ms, ..Default::default() };
    CheckResponse { event: Some(Event::from(result)), ..Default::default() }
}

fn answer_message(answer: Answer) -> LlmAnswer {
    let mut message = LlmAnswer {
        caller_id: answer.caller_id,
        duration_ms: answer.duration.as_millis() as u64,
        ..Default::default()
    };
    match answer.outcome {
        Ok(reply) => {
            message.ok = true;
            (message.json, message.text, message.model) = (reply.json, reply.text, reply.model);
        }
        Err(failure) => (message.error_code, message.error_message) = (failure.code, failure.message),
    }
    message
}

/// Short, stable label for logs and the `visioner.checks` metric.
fn reason(error: &VisionError) -> &'static str {
    match error {
        VisionError::NotConfigured(_) => "not_configured",
        VisionError::Screen(_) => "screen_failed",
        VisionError::AllLlmsFailed { .. } => "all_llms_failed",
    }
}

fn to_connect_error(error: VisionError) -> ConnectError {
    let code = match &error {
        VisionError::NotConfigured(_) => ErrorCode::FailedPrecondition,
        VisionError::Screen(failure) if failure.code == crate::domain::DEADLINE_EXCEEDED => ErrorCode::DeadlineExceeded,
        VisionError::Screen(_) => ErrorCode::Unavailable,
        VisionError::AllLlmsFailed { all_timed_out: true, .. } => ErrorCode::DeadlineExceeded,
        VisionError::AllLlmsFailed { .. } => ErrorCode::Unavailable,
    };
    ConnectError::new(code, error.to_string())
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
