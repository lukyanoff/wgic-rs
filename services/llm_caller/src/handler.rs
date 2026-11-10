//! Proto ⇄ domain for the server-streaming `Call` RPC, plus logs, metrics and error codes.
//!
//! `call` returns a stream at once and runs the LLM call in a background task that pushes
//! progress messages and finally the result into a channel (≈ C# `Channel<T>` feeding an
//! `IAsyncEnumerable<T>`). When the client goes away, the task stops and the driver is dropped,
//! which kills the CLI process or aborts the HTTP request.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use connectrpc::{ConnectError, ErrorCode, RequestContext, Response, ServiceRequest, ServiceResult, ServiceStream};
use contracts::proto::llm_caller::v1::{
    CallMetadata, CallProgress, CallRequest, CallResponse, CallResult, LlmCallerService, call_response::Event,
};
use kernel::{Deps, Logger, Metrics, kv};
use tokio::sync::mpsc;

use crate::domain::{ChatMessage, Image, LlmAnswer, LlmCaller, LlmError, Progress, parse_role};

type Item = Result<CallResponse, ConnectError>;

/// Room for progress messages the client hasn't read yet; when full, an update is skipped
/// (the next one carries the whole partial text anyway).
const STREAM_BUFFER: usize = 64;

// ---------------------------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------------------------

pub struct LlmCallerHandler {
    runner: Arc<CallRunner>,
}

/// Everything a background call needs; shared by all calls of this instance.
struct CallRunner {
    caller: LlmCaller,
    log: Logger,
    metrics: Metrics,
}

impl LlmCallerHandler {
    pub fn new(deps: &Deps, caller: LlmCaller) -> Self {
        let runner = CallRunner { caller, log: deps.logger("llm_caller"), metrics: deps.metrics("llm_caller") };
        Self { runner: Arc::new(runner) }
    }
}

#[allow(refining_impl_trait)]
impl LlmCallerService for LlmCallerHandler {
    async fn call(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, CallRequest>,
    ) -> ServiceResult<ServiceStream<CallResponse>> {
        let request = request.to_owned_message();
        let messages = to_domain(&request).map_err(to_connect_error)?;
        let schema = Some(request.json_schema.trim().to_string()).filter(|s| !s.is_empty());
        let (tx, rx) = mpsc::channel::<Item>(STREAM_BUFFER);
        tokio::spawn(self.runner.clone().run(messages, schema, tx));
        // The stream ends when the task drops `tx` (after the result or error).
        let stream = futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) });
        Response::ok(Box::pin(stream))
    }
}

// ---------------------------------------------------------------------------------------------
// One call, in the background
// ---------------------------------------------------------------------------------------------

impl CallRunner {
    async fn run(self: Arc<Self>, messages: Vec<ChatMessage>, schema: Option<String>, tx: mpsc::Sender<Item>) {
        let report = |progress: Progress| {
            let _ = tx.try_send(Ok(progress_message(progress)));
        };
        let result = tokio::select! {
            result = self.caller.call(messages, schema, &report) => result,
            _ = tx.closed() => {
                self.log.info("llm call cancelled", [kv("caller_id", self.caller.config().id())]);
                return;
            }
        };
        let item = match result {
            Ok(answer) => {
                self.record_success(&answer);
                Ok(self.result_message(answer))
            }
            Err(error) => Err(self.record_failure(error)),
        };
        let _ = tx.send(item).await;
    }

    fn record_success(&self, answer: &LlmAnswer) {
        let config = self.caller.config();
        let tags = [kv("caller_id", config.id()), kv("driver", config.driver()), kv("model", config.model())];
        let millis = answer.duration.as_millis() as u64;
        self.log.info(
            "llm call done",
            tags.iter().cloned().chain([
                kv("input_tokens", answer.reply.input_tokens),
                kv("output_tokens", answer.reply.output_tokens),
                kv("duration_ms", millis),
            ]),
        );
        self.metrics.count("calls", 1, tags.clone());
        self.metrics.record("call_ms", millis as f64, tags.clone());
        self.metrics.count("output_tokens", answer.reply.output_tokens, tags);
    }

    fn record_failure(&self, error: LlmError) -> ConnectError {
        let config = self.caller.config();
        let reason = reason(&error);
        let id = config.id();
        self.log.warn(
            "llm call failed",
            [kv("caller_id", id.as_str()), kv("reason", reason), kv("error", error.to_string())],
        );
        let tags =
            [kv("caller_id", id), kv("driver", config.driver()), kv("model", config.model()), kv("reason", reason)];
        self.metrics.count("failures", 1, tags);
        to_connect_error(error)
    }

    fn result_message(&self, answer: LlmAnswer) -> CallResponse {
        let config = self.caller.config();
        let metadata = CallMetadata {
            caller_id: config.id(),
            driver: config.driver(),
            model: config.model(),
            effort: config.effort(),
            temperature: config.temperature(),
            duration_ms: answer.duration.as_millis() as u64,
            input_tokens: answer.reply.input_tokens,
            output_tokens: answer.reply.output_tokens,
            attributes: answer.reply.attributes.into_iter().collect(),
            ..Default::default()
        };
        let result =
            CallResult { json: answer.json, text: answer.reply.text, metadata: metadata.into(), ..Default::default() };
        CallResponse { event: Some(Event::from(result)), ..Default::default() }
    }
}

// ---------------------------------------------------------------------------------------------
// Mapping (pure)
// ---------------------------------------------------------------------------------------------

fn to_domain(request: &CallRequest) -> Result<Vec<ChatMessage>, LlmError> {
    let mut messages = Vec::new();
    for message in &request.messages {
        let mut domain = ChatMessage::new(parse_role(&message.role)?, message.content.clone());
        for image in &message.images {
            domain.images.push(Image { data: image.data.to_vec(), mime_type: image.mime_type.clone() });
        }
        messages.push(domain);
    }
    Ok(messages)
}

fn progress_message(progress: Progress) -> CallResponse {
    let message = CallProgress {
        stage: progress.stage.name().to_string(),
        partial_text: progress.partial_text,
        elapsed_ms: progress.elapsed.as_millis() as u32,
        ..Default::default()
    };
    CallResponse { event: Some(Event::from(message)), ..Default::default() }
}

/// Short, low-cardinality name of an error, for logs and the `failures` metric.
fn reason(error: &LlmError) -> &'static str {
    match error {
        LlmError::InvalidRequest(_) => "invalid_request",
        LlmError::NotConfigured(_) => "not_configured",
        LlmError::Unauthenticated(_) => "unauthenticated",
        LlmError::RateLimited(_) => "rate_limited",
        LlmError::Unavailable(_) => "unavailable",
        LlmError::Failed(_) => "failed",
        LlmError::InvalidJson { .. } => "invalid_json",
    }
}

fn to_connect_error(error: LlmError) -> ConnectError {
    let code = match &error {
        LlmError::InvalidRequest(_) => ErrorCode::InvalidArgument,
        LlmError::NotConfigured(_) => ErrorCode::FailedPrecondition,
        LlmError::Unauthenticated(_) => ErrorCode::Unauthenticated,
        LlmError::RateLimited(_) => ErrorCode::ResourceExhausted,
        LlmError::Unavailable(_) => ErrorCode::Unavailable,
        LlmError::Failed(_) => ErrorCode::Internal,
        LlmError::InvalidJson { .. } => ErrorCode::DataLoss,
    };
    ConnectError::new(code, error.to_string())
}
