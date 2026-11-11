//! `claude -p` (Claude Code CLI), streaming.
//!
//! In (`--input-format stream-json`): one stdin line holding every image as a base64 block, then the prompt,
//! so no tools or files are needed (`--tools ""`).
//! Out (`--output-format stream-json --verbose --include-partial-messages`), one JSON per line:
//! - `{"type":"stream_event","event":{"delta":{"type":"text_delta","text":"…"}}}` (also `thinking_delta`)
//! - `{"type":"result","is_error":false,"result":"…"}`

use crate::drivers::cli::{CliRun, LineEvent, WorkDir, run_streaming};
use crate::drivers::{Driver, LlmOutput, PromptArgs};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sdk::{ProgressSender, RpcError};
use serde_json::{Value, json};

/// Used when the request has no system prompt (instead of Claude Code's long agent prompt).
const DEFAULT_SYSTEM_PROMPT: &str = "Answer the user's request directly and concisely.";

pub struct ClaudeDriver {
    binary: String,
    /// Empty = the CLI's default model.
    model: String,
}

impl ClaudeDriver {
    pub fn new(binary: String, model: String) -> Self {
        Self { binary, model }
    }

    fn args(&self, args: &PromptArgs) -> Vec<String> {
        let flags = ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"];
        let mut out: Vec<String> = flags.into_iter().map(String::from).collect();
        // No tools and no saved session: a plain prompt → answer call.
        out.extend(["--include-partial-messages", "--tools", "", "--no-session-persistence"].map(String::from));
        if !self.model.is_empty() {
            out.extend(["--model".into(), self.model.clone()]);
        }
        let system = match args.system_prompt.trim() {
            "" => DEFAULT_SYSTEM_PROMPT,
            system => system,
        };
        out.extend(["--system-prompt".into(), system.to_string()]);
        out
    }
}

#[async_trait]
impl Driver for ClaudeDriver {
    async fn call(&self, args: PromptArgs, progress: ProgressSender) -> Result<LlmOutput, RpcError> {
        let work_dir = WorkDir::create(&[])?; // empty: claude gets the images on stdin
        let run = CliRun { program: self.binary.clone(), args: self.args(&args), stdin: Some(stdin_line(&args)) };
        let text = run_streaming(run, work_dir.path(), parse_line, &progress).await?;
        let model = if self.model.is_empty() { "claude".to_string() } else { self.model.clone() };
        Ok(LlmOutput { text, model })
    }
}

/// One `user` line: every image as a base64 block, then the prompt as text.
pub fn stdin_line(args: &PromptArgs) -> String {
    let mut content: Vec<Value> = args
        .images
        .iter()
        .map(|img| {
            let source = json!({"type": "base64", "media_type": img.mime_type, "data": BASE64.encode(&img.data)});
            json!({"type": "image", "source": source})
        })
        .collect();
    content.push(json!({"type": "text", "text": args.user_text()}));
    format!("{}\n", json!({"type": "user", "message": {"role": "user", "content": content}}))
}

pub fn parse_line(line: &str) -> LineEvent {
    let Ok(value) = serde_json::from_str::<Value>(line) else { return LineEvent::Ignore };
    match value["type"].as_str() {
        Some("stream_event") => stream_delta(&value["event"]["delta"]),
        Some("result") => LineEvent::Done(result(&value)),
        _ => LineEvent::Ignore,
    }
}

fn stream_delta(delta: &Value) -> LineEvent {
    match (delta["type"].as_str(), delta["text"].as_str()) {
        (Some("thinking_delta"), _) => LineEvent::Thinking,
        (Some("text_delta"), Some(text)) if !text.is_empty() => LineEvent::Text(text.to_string()),
        _ => LineEvent::Ignore,
    }
}

fn result(value: &Value) -> Result<String, RpcError> {
    let text = value["result"].as_str().unwrap_or_default();
    if value["is_error"].as_bool() == Some(true) {
        return Err(RpcError::internal(format!("claude: {text}")));
    }
    Ok(text.trim().to_string())
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod claude_tests;
