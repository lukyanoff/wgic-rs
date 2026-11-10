//! `claude -p` (Claude Code CLI) as an LLM caller.
//!
//! Input (`--input-format stream-json`): one line on stdin,
//! `{"type":"user","message":{"role":"user","content":[{"type":"image",…}, {"type":"text","text":"…"}]}}`,
//! so images travel as base64 blocks next to the prompt.
//!
//! Output (`--output-format stream-json --verbose --include-partial-messages`), one JSON per line:
//! - `{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"…"}}}`
//!   (also `thinking_delta`, and `input_json_delta` while it fills a `--json-schema` answer)
//! - `{"type":"result","is_error":false,"result":"…","structured_output":{…},"usage":{…},"total_cost_usd":…}`

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

use super::base::{CliFlavor, LineEvent, system_prompt, transcript};
use crate::config::LlmSettings;
use crate::domain::{Delta, LlmError, LlmReply, LlmRequest};

pub struct ClaudeCli;

impl CliFlavor for ClaudeCli {
    fn program(&self) -> &'static str {
        "claude"
    }

    fn args(&self, settings: &LlmSettings, request: &LlmRequest) -> Vec<String> {
        let mut args: Vec<String> =
            ["-p", "--input-format", "stream-json", "--output-format", "stream-json"].map(String::from).to_vec();
        args.extend(["--verbose".into(), "--include-partial-messages".into()]);
        args.extend(["--model".into(), settings.model.clone(), "--effort".into(), settings.effort.clone()]);
        // No tools, no saved session: a plain prompt → answer call.
        args.extend(["--tools".into(), String::new(), "--no-session-persistence".into()]);
        args.extend(["--system-prompt".into(), system_prompt(&request.messages)]);
        if let Some(schema) = &request.json_schema {
            args.extend(["--json-schema".into(), schema.clone()]);
        }
        args
    }

    fn stdin(&self, request: &LlmRequest) -> Option<String> {
        Some(user_line(request))
    }

    fn parse_line(&self, line: &str) -> LineEvent {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return LineEvent::Ignore;
        };
        match value["type"].as_str() {
            Some("stream_event") => stream_delta(&value["event"]["delta"]),
            Some("result") => LineEvent::Done(result(&value)),
            _ => LineEvent::Ignore,
        }
    }
}

/// The stdin line: every image as a base64 block, then the transcript as text.
fn user_line(request: &LlmRequest) -> String {
    let mut content = Vec::new();
    for image in request.messages.iter().flat_map(|m| m.images.iter()) {
        let source = json!({"type": "base64", "media_type": image.mime_type, "data": BASE64.encode(&image.data)});
        content.push(json!({"type": "image", "source": source}));
    }
    content.push(json!({"type": "text", "text": transcript(&request.messages)}));
    let line = json!({"type": "user", "message": {"role": "user", "content": content}});
    format!("{line}\n")
}

fn stream_delta(delta: &Value) -> LineEvent {
    let text = match delta["type"].as_str() {
        Some("thinking_delta") => return LineEvent::Delta(Delta::Thinking),
        Some("text_delta") => delta["text"].as_str(),
        Some("input_json_delta") => delta["partial_json"].as_str(),
        _ => None,
    };
    match text {
        Some(text) if !text.is_empty() => LineEvent::Delta(Delta::Text(text.to_string())),
        _ => LineEvent::Ignore,
    }
}

fn result(value: &Value) -> Result<LlmReply, LlmError> {
    let text = value["result"].as_str().unwrap_or_default();
    if value["is_error"].as_bool() == Some(true) {
        return Err(LlmError::Failed(format!("claude: {text}")));
    }
    let usage = &value["usage"];
    let mut reply = LlmReply {
        // With --json-schema the validated answer is in `structured_output`.
        text: match &value["structured_output"] {
            Value::Null => text.to_string(),
            structured => structured.to_string(),
        },
        input_tokens: ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
            .iter()
            .map(|key| usage[key].as_u64().unwrap_or(0))
            .sum(),
        output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
        ..Default::default()
    };
    if let Some(cost) = value["total_cost_usd"].as_f64() {
        reply.attributes.insert("cost_usd".into(), format!("{cost:.6}"));
    }
    if let Some(session) = value["session_id"].as_str() {
        reply.attributes.insert("session_id".into(), session.to_string());
    }
    Ok(reply)
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
