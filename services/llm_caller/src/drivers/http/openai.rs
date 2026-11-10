//! OpenAI Responses API (`POST /v1/responses`, `stream: true`) as an LLM caller.
//!
//! Events used (each SSE `data:` is a JSON object with a `type`):
//! - `response.output_text.delta` `{"delta":"…"}` → answer text
//! - `response.reasoning_*.delta` → thinking
//! - `response.completed` `{"response":{"usage":{"input_tokens":…,"output_tokens":…}}}`
//! - `response.failed` / `response.incomplete` / `error` → error

use serde_json::{Value, json};

use super::base::{HttpFlavor, SseEvent, messages_json, parse_schema};
use crate::config::LlmSettings;
use crate::domain::{Delta, LlmError, LlmRequest};

pub struct OpenAi;

impl HttpFlavor for OpenAi {
    fn default_base_url(&self) -> &'static str {
        "https://api.openai.com"
    }

    fn path(&self) -> &'static str {
        "/v1/responses"
    }

    fn api_key_env(&self) -> &'static str {
        "OPENAI_API_KEY"
    }

    fn body(&self, settings: &LlmSettings, request: &LlmRequest) -> Value {
        let mut body = json!({
            "model": settings.model,
            "input": messages_json(&request.messages),
            "stream": true,
            "reasoning": { "effort": settings.effort },
            "text": { "format": json_format(request.json_schema.as_deref()) },
        });
        if let Some(temperature) = settings.temperature {
            body["temperature"] = json!(temperature);
        }
        body
    }

    fn parse_event(&self, data: &Value) -> SseEvent {
        let kind = data["type"].as_str().unwrap_or_default();
        match kind {
            "response.output_text.delta" => SseEvent::Delta(Delta::Text(data["delta"].as_str().unwrap_or("").into())),
            "response.completed" => {
                let usage = &data["response"]["usage"];
                SseEvent::Usage {
                    input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
                    output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
                }
            }
            "response.failed" | "response.incomplete" | "error" => SseEvent::Failed(LlmError::Failed(error_text(data))),
            _ if kind.starts_with("response.reasoning") && kind.ends_with(".delta") => SseEvent::Delta(Delta::Thinking),
            _ => SseEvent::Ignore,
        }
    }
}

/// `text.format`: the schema when given (not strict: any JSON Schema is accepted), else any JSON object.
fn json_format(schema: Option<&str>) -> Value {
    match parse_schema(schema) {
        Some(schema) => json!({ "type": "json_schema", "name": "answer", "schema": schema, "strict": false }),
        None => json!({ "type": "json_object" }),
    }
}

fn error_text(data: &Value) -> String {
    let candidates = [
        &data["response"]["error"]["message"],
        &data["response"]["incomplete_details"]["reason"],
        &data["message"],
        &data["error"]["message"],
    ];
    let message = candidates.iter().find_map(|v| v.as_str()).unwrap_or("unknown error");
    format!("openai: {message}")
}

#[cfg(test)]
#[path = "openai_tests.rs"]
mod tests;
