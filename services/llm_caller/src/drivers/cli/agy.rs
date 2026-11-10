//! `agy -p` (Antigravity CLI) as an LLM caller.
//!
//! agy is an agent: by default it may run commands or search the web. We run it sandboxed, in
//! plan mode, in an empty folder, and tell it not to use tools.
//!
//! Images: agy accepts only text on stdin, so each image is written as a file (`image-1.png`, ...) into the
//! call's own folder, and the prompt names those files. agy opens them with one tool step.
//!
//! Output (`--output-format stream-json`), one JSON per line:
//! - `{"event":"step_update","step_update":{"state":"ACTIVE|DONE","step_type":"agent_response","text_delta":"…"}}`
//! - `{"event":"step_update","step_update":{"state":"ACTIVE","step_type":"tool",…}}` (a tool step)
//! - `{"event":"result","result":{"status":"SUCCESS","response":"…","structured_output":{…},"usage":{…}}}`

use serde_json::Value;

use super::base::{CliFlavor, LineEvent, image_files, system_prompt, transcript};
use crate::config::LlmSettings;
use crate::domain::{Delta, LlmError, LlmReply, LlmRequest};

pub struct AgyCli;

impl CliFlavor for AgyCli {
    fn program(&self) -> &'static str {
        "agy"
    }

    fn args(&self, settings: &LlmSettings, request: &LlmRequest) -> Vec<String> {
        let mut args: Vec<String> =
            ["--output-format", "stream-json", "--sandbox", "--mode", "plan"].map(String::from).to_vec();
        args.extend(["--disable-slash-commands".into()]);
        args.extend(["--model".into(), settings.model.clone(), "--effort".into(), settings.effort.clone()]);
        if let Some(schema) = &request.json_schema {
            args.extend(["--json-schema".into(), schema.clone()]);
        }
        // agy has no system-prompt flag and reads the prompt from `-p <prompt>` (last, as it takes a value).
        let prompt = format!(
            "{}{}\n\n{}",
            system_prompt(&request.messages),
            images_note(request),
            transcript(&request.messages)
        );
        args.extend(["-p".into(), prompt]);
        args
    }

    fn stdin(&self, _request: &LlmRequest) -> Option<String> {
        None
    }

    fn files(&self, request: &LlmRequest) -> Vec<(String, Vec<u8>)> {
        let files = image_files(&request.messages);
        files.into_iter().map(|(name, image)| (name, image.data.clone())).collect()
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
fn images_note(request: &LlmRequest) -> String {
    let names: Vec<String> = image_files(&request.messages).into_iter().map(|(name, _)| name).collect();
    if names.is_empty() {
        return String::new();
    }
    format!("\n\nThe attached images are the files {} in the current folder. Open only these files.", names.join(", "))
}

fn step_delta(step: &Value) -> LineEvent {
    // Every step's `text_delta` is new text, including the one on the DONE step.
    if let Some(text) = step["text_delta"].as_str().filter(|t| !t.is_empty()) {
        return LineEvent::Delta(Delta::Text(text.to_string()));
    }
    if step["thinking_delta"].is_string() || step["step_type"].as_str() == Some("tool") {
        return LineEvent::Delta(Delta::Thinking);
    }
    LineEvent::Ignore
}

fn result(result: &Value) -> Result<LlmReply, LlmError> {
    let status = result["status"].as_str().unwrap_or("UNKNOWN");
    if status != "SUCCESS" {
        let error = result["error"].as_str().or(result["response"].as_str()).unwrap_or_default();
        return Err(LlmError::Failed(format!("agy: {status} {error}").trim().to_string()));
    }
    let usage = &result["usage"];
    let mut reply = LlmReply {
        text: match &result["structured_output"] {
            Value::Null => result["response"].as_str().unwrap_or_default().to_string(),
            structured => structured.to_string(),
        },
        input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
        ..Default::default()
    };
    if let Some(id) = result["conversation_id"].as_str() {
        reply.attributes.insert("conversation_id".into(), id.to_string());
    }
    if let Some(turns) = result["num_turns"].as_u64() {
        reply.attributes.insert("num_turns".into(), turns.to_string());
    }
    Ok(reply)
}

#[cfg(test)]
#[path = "agy_tests.rs"]
mod tests;
