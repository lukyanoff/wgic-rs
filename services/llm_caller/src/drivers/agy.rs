//! `AgyCli`: `agy -p` (Antigravity CLI) as a `CliFlavor`.
//!
//! agy is an agent: unasked it may run commands or search the web. So it runs sandboxed, in plan mode, in an
//! empty folder, and the JSON instruction says not to use tools. It reads only text, so each image is written
//! as a file (`image-1.png`, ...) into the call's folder and the prompt names those files.
//!
//! Out (`--output-format stream-json`), one JSON per line:
//! - `{"event":"step_update","step_update":{"step_type":"agent_response","text_delta":"…"}}` (`tool` steps too)
//! - `{"event":"result","result":{"status":"SUCCESS","response":"…","structured_output":{…},"usage":{…}}}`

use connectrpc::ConnectError;
use serde_json::Value;

use super::cli::{CliFlavor, LineEvent, image_files, system_prompt, transcript};
use super::{CallRequest, Delta, LlmReply};
use crate::config::LlmSettings;

pub struct AgyCli;

impl CliFlavor for AgyCli {
    fn program(&self) -> &'static str {
        "agy"
    }

    fn args(&self, settings: &LlmSettings, request: &CallRequest) -> Vec<String> {
        let args = ["--output-format", "stream-json", "--sandbox", "--mode", "plan", "--disable-slash-commands"];
        let mut args: Vec<String> = args.into_iter().map(String::from).collect();
        args.extend(["--model".into(), settings.model.clone(), "--effort".into(), settings.effort.clone()]);
        if !request.json_schema.is_empty() {
            args.extend(["--json-schema".into(), request.json_schema.clone()]);
        }
        // agy has no system-prompt flag; the prompt goes last, after `-p`.
        let system = system_prompt(&request.messages);
        let prompt = format!("{system}{}\n\n{}", images_note(request), transcript(&request.messages));
        args.extend(["-p".into(), prompt]);
        args
    }

    fn stdin(&self, _request: &CallRequest) -> Option<String> {
        None
    }

    fn files(&self, request: &CallRequest) -> Vec<(String, Vec<u8>)> {
        image_files(&request.messages).into_iter().map(|(name, image)| (name, image.data.clone())).collect()
    }

    fn parse_line(&self, line: &str) -> LineEvent {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return LineEvent::Ignore;
        };
        match value["event"].as_str() {
            Some("step_update") => step_delta(&value["step_update"]),
            Some("result") => LineEvent::Done(result(&value["result"])),
            _ => LineEvent::Ignore,
        }
    }
}

/// "The attached images are the files image-1.png, ... in the current folder." (empty without images)
fn images_note(request: &CallRequest) -> String {
    let names: Vec<String> = image_files(&request.messages).into_iter().map(|(name, _)| name).collect();
    if names.is_empty() {
        return String::new();
    }
    format!("\n\nThe attached images are the files {} in the current folder. Open only these files.", names.join(", "))
}

/// Every step's `text_delta` is new answer text (also on the DONE step); tool steps count as thinking.
fn step_delta(step: &Value) -> LineEvent {
    if let Some(text) = step["text_delta"].as_str().filter(|t| !t.is_empty()) {
        return LineEvent::Delta(Delta::Text(text.to_string()));
    }
    if step["thinking_delta"].is_string() || step["step_type"].as_str() == Some("tool") {
        return LineEvent::Delta(Delta::Thinking);
    }
    LineEvent::Ignore
}

fn result(result: &Value) -> Result<LlmReply, ConnectError> {
    let status = result["status"].as_str().unwrap_or("UNKNOWN");
    if status != "SUCCESS" {
        let error = result["error"].as_str().or(result["response"].as_str()).unwrap_or_default();
        return Err(ConnectError::internal(format!("agy: {status} {error}").trim().to_string()));
    }
    let usage = &result["usage"];
    Ok(LlmReply {
        text: match &result["structured_output"] {
            Value::Null => result["response"].as_str().unwrap_or_default().to_string(),
            structured => structured.to_string(),
        },
        input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
    })
}

#[cfg(test)]
#[path = "agy_tests.rs"]
mod tests;
