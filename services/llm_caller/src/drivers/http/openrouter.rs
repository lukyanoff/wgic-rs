//! OpenRouter chat completions (`POST /api/v1/chat/completions`, `stream: true`) as an LLM caller.
//!
//! Events used (each SSE `data:` is a JSON chunk; the stream ends with `data: [DONE]`):
//! - `{"choices":[{"delta":{"content":"…"}}]}` → answer text
//! - `{"choices":[{"delta":{"reasoning":"…"}}]}` → thinking
//! - `{"usage":{"prompt_tokens":…,"completion_tokens":…}}` (last chunk)
//! - `{"error":{"message":"…"}}` → error

use serde_json::{Value, json};

use super::base::{HttpFlavor, SseEvent, messages_json, parse_schema};
use crate::config::LlmSettings;
use crate::domain::{Delta, LlmError, LlmRequest};

pub struct OpenRouter;

impl HttpFlavor for OpenRouter {
    fn default_base_url(&self) -> &'static str {
        "https://openrouter.ai"
    }

    fn path(&self) -> &'static str {
        "/api/v1/chat/completions"
    }

    fn api_key_env(&self) -> &'static str {
        "OPENROUTER_API_KEY"
    }

    fn body(&self, settings: &LlmSettings, request: &LlmRequest) -> Value {
        let mut body = json!({
            "model": settings.model,
            "messages": messages_json(&request.messages),
            "stream": true,
            "reasoning": { "effort": settings.effort },
            "response_format": response_format(request.json_schema.as_deref()),
            "usage": { "include": true },
        });
        if let Some(temperature) = settings.temperature {
            body["temperature"] = json!(temperature);
        }
        body
    }

    fn parse_event(&self, data: &Value) -> SseEvent {
        if let Some(message) = data["error"]["message"].as_str() {
            return SseEvent::Failed(LlmError::Failed(format!("openrouter: {message}")));
        }
        let delta = &data["choices"][0]["delta"];
        if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            return SseEvent::Delta(Delta::Text(text.to_string()));
        }
        if delta["reasoning"].as_str().is_some_and(|t| !t.is_empty()) {
            return SseEvent::Delta(Delta::Thinking);
        }
        let usage = &data["usage"];
        if usage.is_object() {
            let input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0);
            return SseEvent::Usage { input_tokens, output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) };
        }
        SseEvent::Ignore
    }
}

/// `response_format`: the schema when given (not strict), else any JSON object.
fn response_format(schema: Option<&str>) -> Value {
    match parse_schema(schema) {
        Some(schema) => {
            json!({ "type": "json_schema", "json_schema": { "name": "answer", "schema": schema, "strict": false } })
        }
        None => json!({ "type": "json_object" }),
    }
}

#[cfg(test)]
#[path = "openrouter_tests.rs"]
mod tests;
