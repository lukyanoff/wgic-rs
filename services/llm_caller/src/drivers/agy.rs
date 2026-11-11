//! `agy -p` (Antigravity CLI), streaming.
//!
//! agy is an agent: unasked it may run commands. So it runs sandboxed, in plan mode, in an empty folder. It reads
//! only text, so each image is written as a file (`image-1.jpg`, …) and the prompt names those files.
//! Out (`--output-format stream-json`), one JSON per line:
//! - `{"event":"step_update","step_update":{"step_type":"agent_response","text_delta":"…"}}` (`tool` steps too)
//! - `{"event":"result","result":{"status":"SUCCESS","response":"…"}}`

use crate::drivers::cli::{CliRun, LineEvent, WorkDir, images_note, run_streaming};
use crate::drivers::{Driver, LlmOutput, PromptArgs};
use async_trait::async_trait;
use sdk::{ProgressSender, RpcError};
use serde_json::Value;

pub struct AgyDriver {
    binary: String,
    /// Empty = the CLI's default model.
    model: String,
}

impl AgyDriver {
    pub fn new(binary: String, model: String) -> Self {
        Self { binary, model }
    }

    /// agy has no system-prompt flag: the whole prompt goes last, after `-p`.
    pub fn args(&self, prompt: String) -> Vec<String> {
        let flags = ["--output-format", "stream-json", "--sandbox", "--mode", "plan", "--disable-slash-commands"];
        let mut out: Vec<String> = flags.into_iter().map(String::from).collect();
        if !self.model.is_empty() {
            out.extend(["--model".into(), self.model.clone()]);
        }
        out.extend(["-p".into(), prompt]);
        out
    }
}

#[async_trait]
impl Driver for AgyDriver {
    async fn call(&self, args: PromptArgs, progress: ProgressSender) -> Result<LlmOutput, RpcError> {
        let work_dir = WorkDir::create(&args.images)?;
        let prompt = format!("{}{}", args.combined_prompt(), images_note(&work_dir.image_names));
        let run = CliRun { program: self.binary.clone(), args: self.args(prompt), stdin: None };
        let text = run_streaming(run, work_dir.path(), parse_line, &progress).await?;
        let model = if self.model.is_empty() { "agy".to_string() } else { self.model.clone() };
        Ok(LlmOutput { text, model })
    }
}

pub fn parse_line(line: &str) -> LineEvent {
    let Ok(value) = serde_json::from_str::<Value>(line) else { return LineEvent::Ignore };
    match value["event"].as_str() {
        Some("step_update") => step_delta(&value["step_update"]),
        Some("result") => LineEvent::Done(result(&value["result"])),
        _ => LineEvent::Ignore,
    }
}

/// Every step's `text_delta` is new answer text; tool steps and thinking count as thinking.
fn step_delta(step: &Value) -> LineEvent {
    if let Some(text) = step["text_delta"].as_str().filter(|t| !t.is_empty()) {
        return LineEvent::Text(text.to_string());
    }
    if step["thinking_delta"].is_string() || step["step_type"].as_str() == Some("tool") {
        return LineEvent::Thinking;
    }
    LineEvent::Ignore
}

fn result(result: &Value) -> Result<String, RpcError> {
    let status = result["status"].as_str().unwrap_or("UNKNOWN");
    if status != "SUCCESS" {
        let error = result["error"].as_str().or(result["response"].as_str()).unwrap_or_default();
        return Err(RpcError::internal(format!("agy: {status} {error}").trim().to_string()));
    }
    Ok(result["response"].as_str().unwrap_or_default().trim().to_string())
}

#[cfg(test)]
#[path = "agy_tests.rs"]
mod agy_tests;
