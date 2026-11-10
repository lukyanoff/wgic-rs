//! The orchestration rules: one screenshot, K models in parallel, fail safe, everything within timeouts.
//!
//! The domain sees two interfaces and never knows they are RPC clients:
//! - `Screen` (≈ `interface IScreen`): take one screenshot.
//! - `VisionLlm` (≈ `interface IVisionLlm`): ask one model about a picture, reporting progress.
//!
//! `clients.rs` implements both over the generated clients; tests use fakes.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::{BoxFuture, join_all};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

/// The question asked when the request has none.
pub const DEFAULT_QUESTION: &str = "What do you see on the screen?";

/// The JSON every model must answer with.
pub const ANSWER_SCHEMA: &str = r#"{"type":"object","properties":{"summary":{"type":"string"},"details":{"type":"array","items":{"type":"string"}}},"required":["summary","details"]}"#;

/// One screenshot and what is known about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screenshot {
    pub image: Vec<u8>,
    pub info: ScreenInfo,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenInfo {
    pub sensor_id: String,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub bytes: u64,
    pub capture_ms: u32,
}

/// One model's finished answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LlmReply {
    pub json: String,
    pub text: String,
    pub model: String,
}

/// Why a downstream call failed: a Connect code name (`unavailable`, `deadline_exceeded`, ...) + message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFailure {
    pub code: String,
    pub message: String,
}

/// One model's outcome in the result: the reply, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub caller_id: String,
    pub outcome: Result<LlmReply, CallFailure>,
    pub duration: Duration,
}

/// What `check` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub screen: ScreenInfo,
    /// One per model, in configuration order; at least one is `Ok`.
    pub answers: Vec<Answer>,
    pub duration: Duration,
}

/// One progress update for the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// `screen` or the model's instance id.
    pub source: String,
    /// screen: `capturing|captured`; model: `started|thinking|writing|done|failed|timeout`.
    pub stage: String,
    pub partial_text: String,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisionError {
    /// The service has no configuration (or no NATS); the message says what to set.
    NotConfigured(String),
    /// No screenshot: the screen call failed or timed out (`code` = `deadline_exceeded`).
    Screen(CallFailure),
    /// Every model failed. `all_timed_out` when each one ran out of time.
    AllLlmsFailed { summary: String, all_timed_out: bool },
}

// ---------------------------------------------------------------------------------------------
// Interfaces
// ---------------------------------------------------------------------------------------------

/// ≈ `interface IScreen`. The future owns its data (`'static`).
pub trait Screen: Send + Sync {
    fn capture(&self) -> BoxFuture<'static, Result<Screenshot, CallFailure>>;
}

/// ≈ `interface IVisionLlm`.
pub trait VisionLlm: Send + Sync {
    /// The instance id, e.g. `llm-claude`.
    fn id(&self) -> &str;
    /// Ask about `screenshot`; report the model's progress on `progress` while it runs.
    fn ask(&self, question: String, screenshot: Arc<Screenshot>, progress: ProgressSink) -> AskFuture;
}

pub type AskFuture = BoxFuture<'static, Result<LlmReply, CallFailure>>;

/// Where one source reports its progress (≈ C# `IProgress<T>` bound to one source).
#[derive(Clone)]
pub struct ProgressSink {
    source: String,
    started: Instant,
    tx: mpsc::UnboundedSender<Progress>,
}

impl ProgressSink {
    pub fn report(&self, stage: &str, partial_text: &str) {
        let progress = Progress {
            source: self.source.clone(),
            stage: stage.to_string(),
            partial_text: partial_text.to_string(),
            elapsed: self.started.elapsed(),
        };
        let _ = self.tx.send(progress); // nobody listening any more = the check is over
    }
}

// ---------------------------------------------------------------------------------------------
// The orchestrator
// ---------------------------------------------------------------------------------------------

pub struct Visioner {
    screen: Arc<dyn Screen>,
    llms: Vec<Arc<dyn VisionLlm>>,
    screen_timeout: Duration,
    llm_timeout: Duration,
}

impl Visioner {
    pub fn new(screen: Arc<dyn Screen>, llms: Vec<Arc<dyn VisionLlm>>, timeouts: (Duration, Duration)) -> Self {
        Self { screen, llms, screen_timeout: timeouts.0, llm_timeout: timeouts.1 }
    }

    /// Screenshot → all models at once → every answer. Each progress update goes to `report`.
    /// Fails only when there is no screenshot or no model answered.
    pub async fn check(
        &self,
        question: &str,
        report: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<CheckOutcome, VisionError> {
        let started = Instant::now();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sink = |source: &str| ProgressSink { source: source.to_string(), started, tx: tx.clone() };
        let screenshot = self.capture(sink("screen")).await;
        forward_queued(&mut rx, report);
        let screenshot = Arc::new(screenshot?);
        let question = if question.trim().is_empty() { DEFAULT_QUESTION } else { question.trim() };
        let asks = self.llms.iter().map(|llm| self.ask_one(llm.as_ref(), question, screenshot.clone(), sink(llm.id())));
        let mut all = Box::pin(join_all(asks));
        // Forward progress while the models run (≈ `Task.WhenAll` plus a progress loop).
        let answers = loop {
            tokio::select! {
                Some(progress) = rx.recv() => report(progress),
                answers = &mut all => break answers,
            }
        };
        forward_queued(&mut rx, report);
        check_any_ok(&answers)?;
        Ok(CheckOutcome { screen: screenshot.info.clone(), answers, duration: started.elapsed() })
    }

    async fn capture(&self, sink: ProgressSink) -> Result<Screenshot, VisionError> {
        sink.report("capturing", "");
        let screenshot = match tokio::time::timeout(self.screen_timeout, self.screen.capture()).await {
            Ok(Ok(screenshot)) => screenshot,
            Ok(Err(failure)) => return Err(VisionError::Screen(failure)),
            Err(_) => return Err(VisionError::Screen(timed_out("screen", self.screen_timeout))),
        };
        sink.report("captured", "");
        Ok(screenshot)
    }

    /// One model, within `llm_timeout`. On timeout the ask future is dropped, which cancels the call.
    async fn ask_one(
        &self,
        llm: &dyn VisionLlm,
        question: &str,
        screenshot: Arc<Screenshot>,
        sink: ProgressSink,
    ) -> Answer {
        let started = Instant::now();
        let ask = llm.ask(question.to_string(), screenshot, sink.clone());
        let outcome = match tokio::time::timeout(self.llm_timeout, ask).await {
            Ok(outcome) => outcome,
            Err(_) => Err(timed_out(llm.id(), self.llm_timeout)),
        };
        match &outcome {
            Ok(reply) => sink.report("done", &reply.text),
            Err(failure) if failure.code == DEADLINE_EXCEEDED => sink.report("timeout", &failure.message),
            Err(failure) => sink.report("failed", &failure.message),
        }
        Answer { caller_id: llm.id().to_string(), outcome, duration: started.elapsed() }
    }
}

// ---------------------------------------------------------------------------------------------
// Rules (pure)
// ---------------------------------------------------------------------------------------------

pub const DEADLINE_EXCEEDED: &str = "deadline_exceeded";

/// Pass on the updates already queued (a step can report and finish in the same moment).
fn forward_queued(rx: &mut mpsc::UnboundedReceiver<Progress>, report: &(dyn Fn(Progress) + Send + Sync)) {
    while let Ok(progress) = rx.try_recv() {
        report(progress);
    }
}

fn timed_out(what: &str, limit: Duration) -> CallFailure {
    CallFailure { code: DEADLINE_EXCEEDED.into(), message: format!("{what}: no answer within {limit:?}") }
}

/// At least one model answered; otherwise an error that lists each model's failure.
pub fn check_any_ok(answers: &[Answer]) -> Result<(), VisionError> {
    if answers.iter().any(|a| a.outcome.is_ok()) {
        return Ok(());
    }
    let mut parts = Vec::new();
    for answer in answers {
        if let Err(failure) = &answer.outcome {
            parts.push(format!("{}: {} ({})", answer.caller_id, failure.message, failure.code));
        }
    }
    let all_timed_out = answers.iter().all(|a| matches!(&a.outcome, Err(f) if f.code == DEADLINE_EXCEEDED));
    Err(VisionError::AllLlmsFailed { summary: parts.join("; "), all_timed_out })
}

impl fmt::Display for VisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VisionError::NotConfigured(m) => write!(f, "not configured: {m}"),
            VisionError::Screen(failure) => write!(f, "no screenshot: {}", failure.message),
            VisionError::AllLlmsFailed { summary, .. } => write!(f, "every model failed: {summary}"),
        }
    }
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
