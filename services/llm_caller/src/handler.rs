//! `llm_caller.v1.LlmCallerService/Call`, a server stream. The story of one call:
//! 1. check the chat and put the JSON instruction in front of it
//! 2. send `started`, run the driver, and pass its output on as `thinking` / `writing` progress (≤ 4 per second)
//! 3. find the JSON in the answer and send the result
//!
//! Failed calls need no logging here: the platform logs and measures every RPC with its error code.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;
use std::time::{Duration, Instant};

use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceResult, ServiceStream};
use contracts::proto::llm_caller::v1::{
    CallMetadata, CallProgress, CallRequest, CallResponse, CallResult, ChatMessage, LlmCallerService,
    call_response::Event,
};
use kernel::{Deps, Logger, Metrics, StreamTx, kv, stream_from};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::config::LlmSettings;
use crate::drivers::{Delta, LlmDriver, LlmReply};

/// Progress is sent at most this often (plus on every stage change), so a fast model doesn't send one
/// message per token.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// A call that takes longer is stopped, even when the client set no deadline.
const MAX_CALL: Duration = Duration::from_secs(600);

// ---------------------------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------------------------

/// Cheap to clone (strings, `Arc`s), so each call's stream can own a copy.
#[derive(Clone)]
pub struct LlmCallerHandler {
    settings: LlmSettings,
    driver: Arc<dyn LlmDriver>,
    log: Logger,
    metrics: Metrics,
}

impl LlmCallerHandler {
    pub fn new(deps: &Deps, settings: LlmSettings, driver: Arc<dyn LlmDriver>) -> Self {
        Self { settings, driver, log: deps.logger("llm_caller"), metrics: deps.metrics("llm_caller") }
    }
}

#[allow(refining_impl_trait)]
impl LlmCallerService for LlmCallerHandler {
    async fn call(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, CallRequest>,
    ) -> ServiceResult<ServiceStream<CallResponse>> {
        let request = with_json_instruction(checked(request.to_owned_message())?);
        let handler = self.clone();
        Response::ok(stream_from(move |tx| handler.run(request, tx)))
    }
}

impl LlmCallerHandler {
    /// Runs inside the response stream: when the client goes away, this stops and the driver call is dropped.
    async fn run(self, request: CallRequest, tx: StreamTx<CallResponse>) {
        let started = Instant::now();
        let mut progress = Progress::new(started);
        tx.send(progress_message(progress.update(started)));
        let reply = match tokio::time::timeout(MAX_CALL, self.drive(request, &mut progress, &tx)).await {
            Ok(Ok(reply)) => reply,
            Ok(Err(error)) => return tx.fail(error),
            Err(_) => return tx.fail(ConnectError::deadline_exceeded(format!("no answer within {MAX_CALL:?}"))),
        };
        let Some(json) = extract_json(&reply.text) else {
            let excerpt: String = reply.text.chars().take(300).collect();
            return tx.fail(ConnectError::data_loss(format!("the answer is not JSON: {excerpt}")));
        };
        tx.send(self.result_message(json, reply, started.elapsed()));
    }

    /// Run the driver and pass its output on as progress at the same time (≈ `Task.WhenAny` in a loop).
    async fn drive(
        &self,
        request: CallRequest,
        progress: &mut Progress,
        tx: &StreamTx<CallResponse>,
    ) -> Result<LlmReply, ConnectError> {
        let (deltas, mut received) = mpsc::unbounded_channel();
        let mut call = self.driver.call(request, deltas);
        let reply = loop {
            tokio::select! {
                Some(delta) = received.recv() => progress.add(delta, tx),
                result = &mut call => break result?,
            }
        };
        // A driver can report its last pieces and finish in the same moment: pass those on too.
        while let Ok(delta) = received.try_recv() {
            progress.add(delta, tx);
        }
        Ok(reply)
    }

    /// The final message; also logs the call and counts its output tokens.
    fn result_message(&self, json: String, reply: LlmReply, duration: Duration) -> CallResponse {
        let m = CallMetadata {
            caller_id: self.settings.id.clone(),
            driver: self.settings.driver.clone(),
            model: self.settings.model.clone(),
            duration_ms: duration.as_millis() as u64,
            input_tokens: reply.input_tokens,
            output_tokens: reply.output_tokens,
            ..Default::default()
        };
        let fields = [
            kv("caller_id", m.caller_id.as_str()),
            kv("driver", m.driver.as_str()),
            kv("model", m.model.as_str()),
            kv("duration_ms", m.duration_ms),
            kv("input_tokens", m.input_tokens),
            kv("output_tokens", m.output_tokens),
        ];
        self.log.info("llm call done", fields);
        let tags = [kv("driver", m.driver.as_str()), kv("model", m.model.as_str())];
        self.metrics.count("output_tokens", m.output_tokens, tags);

        let result = CallResult { json, text: reply.text, metadata: m.into(), ..Default::default() };
        CallResponse { event: Some(Event::from(result)), ..Default::default() }
    }
}

// ---------------------------------------------------------------------------------------------
// Progress (throttling, unit tested with explicit times)
// ---------------------------------------------------------------------------------------------

/// Collects the model's output into the partial text and decides when to send an update.
struct Progress {
    started: Instant,
    stage: &'static str,
    partial_text: String,
    last_sent: Instant,
}

impl Progress {
    fn new(started: Instant) -> Self {
        Self { started, stage: "started", partial_text: String::new(), last_sent: started }
    }

    /// Add one delta and send an update when one is due.
    fn add(&mut self, delta: Delta, tx: &StreamTx<CallResponse>) {
        if let Some(update) = self.next(delta, Instant::now()) {
            tx.send(progress_message(update));
        }
    }

    /// Add one delta; `Some(update)` when the stage changed or `PROGRESS_INTERVAL` has passed.
    fn next(&mut self, delta: Delta, now: Instant) -> Option<CallProgress> {
        let stage = match delta {
            Delta::Thinking => "thinking",
            Delta::Text(text) => {
                self.partial_text.push_str(&text);
                "writing"
            }
        };
        let stage_changed = stage != self.stage;
        self.stage = stage;
        if !stage_changed && now.duration_since(self.last_sent) < PROGRESS_INTERVAL {
            return None;
        }
        self.last_sent = now;
        Some(self.update(now))
    }

    fn update(&self, now: Instant) -> CallProgress {
        let elapsed_ms = now.duration_since(self.started).as_millis() as u32;
        CallProgress {
            stage: self.stage.into(),
            partial_text: self.partial_text.clone(),
            elapsed_ms,
            ..Default::default()
        }
    }
}

fn progress_message(progress: CallProgress) -> CallResponse {
    CallResponse { event: Some(Event::from(progress)), ..Default::default() }
}

// ---------------------------------------------------------------------------------------------
// The chat and the JSON (pure, unit tested)
// ---------------------------------------------------------------------------------------------

/// Roles are `system` / `user` / `assistant` (any case, stored lowercase); at least one user message has text;
/// the schema, when given, is JSON. Anything else is `invalid_argument`.
fn checked(mut request: CallRequest) -> Result<CallRequest, ConnectError> {
    for message in &mut request.messages {
        message.role = message.role.trim().to_ascii_lowercase();
        if !["system", "user", "assistant"].contains(&message.role.as_str()) {
            let error = format!("role {:?} must be system, user or assistant", message.role);
            return Err(ConnectError::invalid_argument(error));
        }
    }
    if !request.messages.iter().any(|m| m.role == "user" && !m.content.trim().is_empty()) {
        return Err(ConnectError::invalid_argument("messages need at least one non-empty user message"));
    }
    request.json_schema = request.json_schema.trim().to_string();
    if !request.json_schema.is_empty() && serde_json::from_str::<Value>(&request.json_schema).is_err() {
        return Err(ConnectError::invalid_argument("json_schema is not valid JSON"));
    }
    Ok(request)
}

/// Put a system message in front that asks for JSON only (with the schema, when there is one).
/// With images, a model that gets them as files (agy) may open those files, and nothing else.
fn with_json_instruction(mut request: CallRequest) -> CallRequest {
    let has_images = request.messages.iter().any(|m| !m.images.is_empty());
    let mut text = String::from("Answer with JSON only: no prose, no markdown fences. ");
    text.push_str(match has_images {
        true => "Look at the attached images; you may open the attached image files, but do not use any other tool.",
        false => "Answer directly from your own knowledge; do not use tools.",
    });
    match request.json_schema.as_str() {
        "" => text.push_str("\nUse a JSON object or array."),
        schema => text.push_str(&format!("\nThe JSON must match this JSON Schema:\n{schema}")),
    }
    let instruction = ChatMessage { role: "system".into(), content: text, ..Default::default() };
    request.messages.insert(0, instruction);
    request
}

/// The JSON document in `text`, compact. Models sometimes wrap it in ```json fences or write a sentence
/// first, so this tries the whole text, then the part from the first `{`/`[` to the last `}`/`]`.
fn extract_json(text: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
        return Some(value.to_string());
    }
    let start = text.find(['{', '['])?;
    let end = text.rfind(['}', ']'])?;
    if end < start {
        return None;
    }
    let value = serde_json::from_str::<Value>(&text[start..=end]).ok()?;
    Some(value.to_string())
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
