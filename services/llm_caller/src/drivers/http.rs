//! `HttpDriver`: OpenAI and OpenRouter. Both speak the OpenAI chat-completions API, so one class with two
//! presets is enough: POST `/chat/completions` with `stream: true`, then read Server-Sent Events.
//!
//! Events (each `data:` line is a JSON chunk; the stream ends with `data: [DONE]`):
//! - `{"choices":[{"delta":{"content":"…"}}]}` → answer text
//! - `{"choices":[{"delta":{"reasoning":"…"}}]}` (OpenRouter) → thinking
//! - `{"usage":{"prompt_tokens":…,"completion_tokens":…}}` → tokens (last chunk)
//! - `{"error":{"message":"…"}}` → error

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use connectrpc::ConnectError;
use futures::StreamExt;
use kernel::Deps;
use serde_json::{Value, json};

use super::{CallRequest, ChatMessage, Delta, Deltas, DriverFuture, LlmDriver, LlmReply};
use crate::config::LlmSettings;

// ---------------------------------------------------------------------------------------------
// The driver
// ---------------------------------------------------------------------------------------------

/// Cheap to clone, so every call's future can own its copy.
#[derive(Clone)]
pub struct HttpDriver {
    /// E.g. `https://api.openai.com/v1/chat/completions`.
    url: String,
    /// `None` → every call answers `failed_precondition` (the host still starts).
    api_key: Option<String>,
    api_key_env: &'static str,
    model: String,
    effort: String,
    client: reqwest::Client,
}

impl HttpDriver {
    /// The preset for `settings.driver` (`openai` or `openrouter`), with the API key from the environment.
    pub fn new(deps: &Deps, settings: &LlmSettings) -> Self {
        let (default_base, api_key_env) = match settings.driver.as_str() {
            "openrouter" => ("https://openrouter.ai/api/v1", "OPENROUTER_API_KEY"),
            _ => ("https://api.openai.com/v1", "OPENAI_API_KEY"),
        };
        let base = settings.base_url.as_deref().unwrap_or(default_base);
        Self {
            url: format!("{base}/chat/completions"),
            api_key: deps.env(api_key_env),
            api_key_env,
            model: settings.model.clone(),
            effort: settings.effort.clone(),
            client: reqwest::Client::new(),
        }
    }

    async fn run(self, request: CallRequest, deltas: Deltas) -> Result<LlmReply, ConnectError> {
        let Some(api_key) = &self.api_key else {
            return Err(ConnectError::failed_precondition(format!("{} is not set", self.api_key_env)));
        };
        let body = self.body(&request);
        let sent = self.client.post(&self.url).bearer_auth(api_key).json(&body).send().await;
        let response = sent.map_err(|e| ConnectError::unavailable(format!("POST {}: {e}", self.url)))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(status_error(status, &response.text().await.unwrap_or_default()));
        }

        // Read SSE lines until `data: [DONE]` or the end of the stream, collecting text and usage.
        let mut reply = LlmReply::default();
        let mut buffer: Vec<u8> = Vec::new();
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|e| ConnectError::unavailable(format!("read stream: {e}")))?;
            buffer.extend_from_slice(&chunk);
            if apply_lines(&mut buffer, &mut reply, &deltas)? {
                break;
            }
        }
        Ok(reply)
    }

    fn body(&self, request: &CallRequest) -> Value {
        let mut messages = Vec::new();
        for message in &request.messages {
            messages.push(json!({ "role": message.role, "content": content(message) }));
        }
        json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "reasoning_effort": self.effort,
            "response_format": response_format(&request.json_schema),
        })
    }
}

impl LlmDriver for HttpDriver {
    fn call(&self, request: CallRequest, deltas: Deltas) -> DriverFuture {
        Box::pin(self.clone().run(request, deltas))
    }
}

// ---------------------------------------------------------------------------------------------
// Pure helpers (unit tested)
// ---------------------------------------------------------------------------------------------

/// A text-only message stays a string; with images it becomes parts: the images as `data:` URLs, then the text.
fn content(message: &ChatMessage) -> Value {
    if message.images.is_empty() {
        return json!(message.content);
    }
    let mut parts = Vec::new();
    for image in &message.images {
        let url = format!("data:{};base64,{}", image.mime_type, BASE64.encode(&image.data));
        parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
    }
    parts.push(json!({ "type": "text", "text": message.content }));
    Value::Array(parts)
}

/// The schema when there is one (not strict: any JSON Schema is accepted), else any JSON object.
fn response_format(schema: &str) -> Value {
    match serde_json::from_str::<Value>(schema) {
        Ok(schema) => {
            json!({ "type": "json_schema", "json_schema": { "name": "answer", "schema": schema, "strict": false } })
        }
        Err(_) => json!({ "type": "json_object" }),
    }
}

/// Apply one `data:` chunk: report a delta, keep the text and the token counts, or fail.
fn apply(data: &str, reply: &mut LlmReply, deltas: &Deltas) -> Result<(), ConnectError> {
    let Ok(chunk) = serde_json::from_str::<Value>(data) else {
        return Ok(()); // keep-alive comments and the like
    };
    if let Some(message) = chunk["error"]["message"].as_str() {
        return Err(ConnectError::internal(format!("provider error: {message}")));
    }
    let delta = &chunk["choices"][0]["delta"];
    if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
        reply.text.push_str(text);
        let _ = deltas.send(Delta::Text(text.to_string()));
    } else if delta["reasoning"].as_str().is_some_and(|t| !t.is_empty()) || delta["reasoning_details"].is_array() {
        let _ = deltas.send(Delta::Thinking);
    }
    if chunk["usage"].is_object() {
        reply.input_tokens = chunk["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
        reply.output_tokens = chunk["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    }
    Ok(())
}

/// Apply every complete `data:` line in `buffer`; `true` once the stream says `[DONE]`.
fn apply_lines(buffer: &mut Vec<u8>, reply: &mut LlmReply, deltas: &Deltas) -> Result<bool, ConnectError> {
    while let Some(line) = take_line(buffer) {
        match line.strip_prefix("data:").map(str::trim) {
            Some("[DONE]") => return Ok(true),
            Some(data) => apply(data, reply, deltas)?,
            None => {} // `event:` lines, comments, blank separators
        }
    }
    Ok(false)
}

/// Remove and return the first complete line (without `\r\n`), or `None` if there is none yet.
/// Works on bytes, so a UTF-8 character split across two network chunks stays intact.
fn take_line(buffer: &mut Vec<u8>) -> Option<String> {
    let end = buffer.iter().position(|&b| b == b'\n')?;
    let line: Vec<u8> = buffer.drain(..=end).collect();
    Some(String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']).to_string())
}

/// HTTP error status → Connect error, with the provider's message when it sent one.
fn status_error(status: u16, body: &str) -> ConnectError {
    let message = match serde_json::from_str::<Value>(body) {
        Ok(value) if value["error"]["message"].is_string() => {
            value["error"]["message"].as_str().unwrap_or("").to_string()
        }
        _ => body.chars().take(300).collect(),
    };
    let message = format!("HTTP {status}: {message}");
    match status {
        401 | 403 => ConnectError::unauthenticated(message),
        429 => ConnectError::resource_exhausted(message),
        500.. => ConnectError::unavailable(message),
        _ => ConnectError::internal(message),
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
