//! The LLM caller "base class".
//!
//! Rust has no inheritance, so the C# `abstract class LlmCallerBase` is split in two:
//! - `trait LlmDriver` — the abstract member each caller implements (`call`: send the chat, report
//!   deltas, return the text). Implementations: `drivers/cli/*` and `drivers/http/*`.
//! - `struct LlmCaller` — the shared body: validate the chat, ask for JSON, throttle progress,
//!   extract + check the JSON, time the call.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::config::LlmConfig;
use crate::json::{excerpt, extract_json, json_instruction};

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// Pictures for the model to look at (user messages only).
    pub images: Vec<Image>,
}

impl ChatMessage {
    /// A text-only message.
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self { role, content: content.into(), images: Vec::new() }
    }
}

/// One picture sent with a message, e.g. a screenshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub data: Vec<u8>,
    /// `image/png` | `image/jpeg` | `image/webp` | `image/gif`
    pub mime_type: String,
}

/// Image types every driver accepts.
pub const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

/// Largest image accepted (the Anthropic limit; the others allow more).
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// What a driver gets: the chat (the JSON instruction is already in it) and the optional schema,
/// for providers that can enforce it natively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRequest {
    pub messages: Vec<ChatMessage>,
    pub json_schema: Option<String>,
}

/// One piece of streamed output, as reported by a driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    /// The model is reasoning (the text is not part of the answer).
    Thinking,
    /// New answer text.
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Started,
    Thinking,
    Writing,
}

/// One progress update sent to the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    /// Everything written so far.
    pub partial_text: String,
    pub elapsed: Duration,
}

/// What a driver returns when the model is done.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LlmReply {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Driver-specific facts, e.g. `cost_usd`, `session_id`.
    pub attributes: BTreeMap<String, String>,
}

/// The final result of one call.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmAnswer {
    /// Compact JSON, already validated.
    pub json: String,
    pub reply: LlmReply,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// Bad chat in the request (empty, unknown role).
    InvalidRequest(String),
    /// The instance can't call at all: missing API key, CLI not installed.
    NotConfigured(String),
    Unauthenticated(String),
    RateLimited(String),
    /// Provider down, network error or timeout.
    Unavailable(String),
    /// The provider or CLI reported an error.
    Failed(String),
    /// The model answered, but not with JSON.
    InvalidJson {
        excerpt: String,
    },
}

/// The async result of a driver call (≈ C# `Task<LlmReply>`), boxed so `LlmDriver` can be a `dyn` interface.
pub type DriverFuture = Pin<Box<dyn Future<Output = Result<LlmReply, LlmError>> + Send>>;

// ---------------------------------------------------------------------------------------------
// Interface: what each caller implements
// ---------------------------------------------------------------------------------------------

/// ≈ C# `interface ILlmDriver`. The future must own its data (`'static`): clone what you need.
pub trait LlmDriver: Send + Sync {
    /// Send the chat; report output pieces on `deltas` as they arrive; return the full text.
    fn call(&self, request: LlmRequest, deltas: mpsc::UnboundedSender<Delta>) -> DriverFuture;
}

// ---------------------------------------------------------------------------------------------
// The base class body
// ---------------------------------------------------------------------------------------------

/// Progress is sent at most this often (plus on every stage change), so a fast model
/// doesn't flood the client with one message per token.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub struct LlmCaller {
    config: Arc<dyn LlmConfig>,
    driver: Arc<dyn LlmDriver>,
}

impl LlmCaller {
    pub fn new(config: Arc<dyn LlmConfig>, driver: Arc<dyn LlmDriver>) -> Self {
        Self { config, driver }
    }

    pub fn config(&self) -> &dyn LlmConfig {
        self.config.as_ref()
    }

    /// Run one call. Each progress update is passed to `report` (≈ C# `IProgress<Progress>.Report`);
    /// the JSON answer is returned.
    pub async fn call(
        &self,
        messages: Vec<ChatMessage>,
        json_schema: Option<String>,
        report: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<LlmAnswer, LlmError> {
        let request = prepare(messages, json_schema)?;
        let started = Instant::now();
        let mut tracker = ProgressTracker::new(started);
        report(tracker.started());
        let (delta_tx, mut delta_rx) = mpsc::unbounded_channel();
        let mut driver_call = self.driver.call(request, delta_tx);
        // Run the driver and forward its deltas at the same time (≈ C# `Task.WhenAny` in a loop).
        let reply = loop {
            tokio::select! {
                Some(delta) = delta_rx.recv() => tracker.report(delta, report),
                result = &mut driver_call => break result?,
            }
        };
        // A driver can send its last pieces and finish in the same step: report those too.
        while let Ok(delta) = delta_rx.try_recv() {
            tracker.report(delta, report);
        }
        let Some(json) = extract_json(&reply.text) else {
            return Err(LlmError::InvalidJson { excerpt: excerpt(&reply.text, 300) });
        };
        Ok(LlmAnswer { json, reply, duration: started.elapsed() })
    }
}

/// Check the chat and the schema, and add the JSON instruction as the first system message.
pub fn prepare(messages: Vec<ChatMessage>, json_schema: Option<String>) -> Result<LlmRequest, LlmError> {
    if !messages.iter().any(|m| m.role == Role::User && !m.content.trim().is_empty()) {
        return Err(LlmError::InvalidRequest("messages need at least one non-empty user message".into()));
    }
    if let Some(schema) = &json_schema
        && serde_json::from_str::<serde_json::Value>(schema).is_err()
    {
        return Err(LlmError::InvalidRequest("json_schema is not valid JSON".into()));
    }
    check_images(&messages)?;
    let has_images = messages.iter().any(|m| !m.images.is_empty());
    let instruction = ChatMessage::new(Role::System, json_instruction(json_schema.as_deref(), has_images));
    let mut all = vec![instruction];
    all.extend(messages);
    Ok(LlmRequest { messages: all, json_schema })
}

/// Images only on user messages, of a known type, not empty and not too big.
fn check_images(messages: &[ChatMessage]) -> Result<(), LlmError> {
    for message in messages {
        if !message.images.is_empty() && message.role != Role::User {
            return Err(LlmError::InvalidRequest("images are allowed on user messages only".into()));
        }
        for image in &message.images {
            if !IMAGE_TYPES.contains(&image.mime_type.as_str()) {
                let allowed = IMAGE_TYPES.join(", ");
                return Err(LlmError::InvalidRequest(format!(
                    "image type {:?} is not one of {allowed}",
                    image.mime_type
                )));
            }
            if image.data.is_empty() || image.data.len() > MAX_IMAGE_BYTES {
                return Err(LlmError::InvalidRequest(format!("an image must have 1..{MAX_IMAGE_BYTES} bytes")));
            }
        }
    }
    Ok(())
}

/// `system` | `user` | `assistant` (case-insensitive).
pub fn parse_role(value: &str) -> Result<Role, LlmError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "system" => Ok(Role::System),
        "user" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        _ => Err(LlmError::InvalidRequest(format!("role {value:?} must be system, user or assistant"))),
    }
}

// ---------------------------------------------------------------------------------------------
// Progress throttling (pure, unit tested with explicit times)
// ---------------------------------------------------------------------------------------------

/// Collects deltas into the partial text and decides when to send an update.
pub struct ProgressTracker {
    started: Instant,
    partial_text: String,
    stage: Stage,
    last_sent: Instant,
}

impl ProgressTracker {
    pub fn new(started: Instant) -> Self {
        Self { started, partial_text: String::new(), stage: Stage::Started, last_sent: started }
    }

    pub fn started(&self) -> Progress {
        Progress { stage: Stage::Started, partial_text: String::new(), elapsed: Duration::ZERO }
    }

    /// Add one delta and pass the update to `report` when one is due.
    pub fn report(&mut self, delta: Delta, report: &(dyn Fn(Progress) + Send + Sync)) {
        if let Some(update) = self.add(delta, Instant::now()) {
            report(update);
        }
    }

    /// Add one delta; `Some(update)` when the stage changed or `PROGRESS_INTERVAL` passed.
    pub fn add(&mut self, delta: Delta, now: Instant) -> Option<Progress> {
        let stage = match delta {
            Delta::Thinking => Stage::Thinking,
            Delta::Text(text) => {
                self.partial_text.push_str(&text);
                Stage::Writing
            }
        };
        let stage_changed = stage != self.stage;
        self.stage = stage;
        if !stage_changed && now.duration_since(self.last_sent) < PROGRESS_INTERVAL {
            return None;
        }
        self.last_sent = now;
        Some(Progress { stage, partial_text: self.partial_text.clone(), elapsed: now.duration_since(self.started) })
    }
}

// ---------------------------------------------------------------------------------------------
// Display (messages of errors)
// ---------------------------------------------------------------------------------------------

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::InvalidRequest(m) => write!(f, "invalid request: {m}"),
            LlmError::NotConfigured(m) => write!(f, "not configured: {m}"),
            LlmError::Unauthenticated(m) => write!(f, "unauthenticated: {m}"),
            LlmError::RateLimited(m) => write!(f, "rate limited: {m}"),
            LlmError::Unavailable(m) => write!(f, "unavailable: {m}"),
            LlmError::Failed(m) => write!(f, "failed: {m}"),
            LlmError::InvalidJson { excerpt } => write!(f, "the answer is not JSON: {excerpt}"),
        }
    }
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Started => "started",
            Stage::Thinking => "thinking",
            Stage::Writing => "writing",
        }
    }
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
