//! `ClaudeCli`: `claude -p` (Claude Code CLI) as a `CliFlavor`.
//!
//! In (`--input-format stream-json`): one stdin line holding every image as a base64 block, then the prompt.
//! Out (`--output-format stream-json --verbose --include-partial-messages`), one JSON per line:
//! - `{"type":"stream_event","event":{"delta":{"type":"text_delta","text":"…"}}}` (also `thinking_delta`, and
//!   `input_json_delta` while it fills a `--json-schema` answer)
//! - `{"type":"result","is_error":false,"result":"…","structured_output":{…},"usage":{…}}`

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use connectrpc::ConnectError;
use serde_json::{Value, json};

use super::cli::{CliFlavor, LineEvent, system_prompt, transcript};
use super::{CallRequest, Delta, LlmReply};
use crate::config::LlmSettings;

pub struct ClaudeCli;

impl CliFlavor for ClaudeCli {
    fn program(&self) -> &'static str {
        "claude"
    }

    fn args(&self, settings: &LlmSettings, request: &CallRequest) -> Vec<String> {
        let mut args = vec!["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"];
        // No tools and no saved session: a plain prompt → answer call.
        args.extend(["--include-partial-messages", "--tools", "", "--no-session-persistence"]);
        let mut args: Vec<String> = args.into_iter().map(String::from).collect();
        args.extend(["--model".into(), settings.model.clone(), "--effort".into(), settings.effort.clone()]);
        args.extend(["--system-prompt".into(), system_prompt(&request.messages)]);
        if !request.json_schema.is_empty() {
            args.extend(["--json-schema".into(), request.json_schema.clone()]);
        }
        args
    }

    /// One `user` line: every image as a base64 block, then the transcript as text.
    fn stdin(&self, request: &CallRequest) -> Option<String> {
        let mut content = Vec::new();
        for image in request.messages.iter().flat_map(|m| &m.images) {
            let source = json!({"type": "base64", "media_type": image.mime_type, "data": BASE64.encode(&image.data)});
            content.push(json!({"type": "image", "source": source}));
        }
        content.push(json!({"type": "text", "text": transcript(&request.messages)}));
        Some(format!("{}\n", json!({"type": "user", "message": {"role": "user", "content": content}})))
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

fn result(value: &Value) -> Result<LlmReply, ConnectError> {
    let text = value["result"].as_str().unwrap_or_default();
    if value["is_error"].as_bool() == Some(true) {
        return Err(ConnectError::internal(format!("claude: {text}")));
    }
    let usage = &value["usage"];
    let tokens = |key: &str| usage[key].as_u64().unwrap_or(0);
    Ok(LlmReply {
        // With --json-schema the validated answer is in `structured_output`.
        text: match &value["structured_output"] {
            Value::Null => text.to_string(),
            structured => structured.to_string(),
        },
        input_tokens: tokens("input_tokens")
            + tokens("cache_creation_input_tokens")
            + tokens("cache_read_input_tokens"),
        output_tokens: tokens("output_tokens"),
    })
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
