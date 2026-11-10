//! `HttpCaller`: the shared "base class" of the HTTP drivers (OpenAI, OpenRouter).
//!
//! POSTs a streaming request with the API key, reads the Server-Sent Events (`data: {...}` lines)
//! and hands each event to the flavor (the "subclass") to interpret. Collects the answer text.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::config::LlmSettings;
use crate::domain::{ChatMessage, Delta, DriverFuture, Image, LlmDriver, LlmError, LlmReply, LlmRequest, Role};
use crate::json::excerpt;

// ---------------------------------------------------------------------------------------------
// Interface: what differs per provider (≈ the abstract members of `HttpCallerBase`)
// ---------------------------------------------------------------------------------------------

/// What one SSE event means.
#[derive(Debug, Clone, PartialEq)]
pub enum SseEvent {
    Ignore,
    Delta(Delta),
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    /// The provider reported an error inside the stream.
    Failed(LlmError),
}

pub trait HttpFlavor: Send + Sync {
    /// E.g. `https://api.openai.com`; `LLM_CALLER_BASE_URL` replaces it.
    fn default_base_url(&self) -> &'static str;
    /// E.g. `/v1/responses`.
    fn path(&self) -> &'static str;
    /// E.g. `OPENAI_API_KEY`.
    fn api_key_env(&self) -> &'static str;
    /// The JSON request body for one call.
    fn body(&self, settings: &LlmSettings, request: &LlmRequest) -> Value;
    /// Interpret one `data:` payload (already parsed as JSON).
    fn parse_event(&self, data: &Value) -> SseEvent;
}

// ---------------------------------------------------------------------------------------------
// The base class body
// ---------------------------------------------------------------------------------------------

/// Cheap to clone (`Arc`s + settings), so each call's future can own a copy.
#[derive(Clone)]
pub struct HttpCaller {
    flavor: Arc<dyn HttpFlavor>,
    settings: LlmSettings,
    /// `None` → every call answers `failed_precondition` (the host still starts).
    api_key: Option<String>,
    client: reqwest::Client,
}

impl HttpCaller {
    pub fn new(flavor: Arc<dyn HttpFlavor>, settings: LlmSettings, api_key: Option<String>) -> Self {
        Self { flavor, settings, api_key, client: reqwest::Client::new() }
    }

    async fn run(self, request: LlmRequest, deltas: mpsc::UnboundedSender<Delta>) -> Result<LlmReply, LlmError> {
        match tokio::time::timeout(self.settings.timeout, self.send(request, deltas)).await {
            Ok(result) => result,
            Err(_) => Err(LlmError::Unavailable(format!("no answer within {:?}", self.settings.timeout))),
        }
    }

    async fn send(&self, request: LlmRequest, deltas: mpsc::UnboundedSender<Delta>) -> Result<LlmReply, LlmError> {
        let Some(api_key) = &self.api_key else {
            return Err(LlmError::NotConfigured(format!("{} is not set", self.flavor.api_key_env())));
        };
        let base = self.settings.base_url.as_deref().unwrap_or(self.flavor.default_base_url());
        let url = format!("{base}{}", self.flavor.path());
        let body = self.flavor.body(&self.settings, &request);
        let sent = self.client.post(&url).bearer_auth(api_key).json(&body).send().await;
        let response = sent.map_err(|e| LlmError::Unavailable(format!("POST {url}: {e}")))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let text = response.text().await.unwrap_or_default();
            return Err(status_error(status, &text));
        }
        self.read_events(response, deltas).await
    }

    /// Read SSE lines until the stream ends (or `data: [DONE]`), collecting text and usage.
    async fn read_events(
        &self,
        response: reqwest::Response,
        deltas: mpsc::UnboundedSender<Delta>,
    ) -> Result<LlmReply, LlmError> {
        let mut reply = LlmReply::default();
        let mut buffer: Vec<u8> = Vec::new();
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|e| LlmError::Unavailable(format!("read stream: {e}")))?;
            buffer.extend_from_slice(&chunk);
            let finished = self.apply_lines(&mut buffer, &mut reply, &deltas)?;
            if finished {
                return Ok(reply);
            }
        }
        Ok(reply)
    }

    /// Apply every complete line in `buffer`. `true` once `data: [DONE]` arrived.
    fn apply_lines(
        &self,
        buffer: &mut Vec<u8>,
        reply: &mut LlmReply,
        deltas: &mpsc::UnboundedSender<Delta>,
    ) -> Result<bool, LlmError> {
        while let Some(line) = take_line(buffer) {
            match sse_data(&line) {
                None => {}
                Some("[DONE]") => return Ok(true),
                Some(data) => self.apply(data, reply, deltas)?,
            }
        }
        Ok(false)
    }

    fn apply(&self, data: &str, reply: &mut LlmReply, deltas: &mpsc::UnboundedSender<Delta>) -> Result<(), LlmError> {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return Ok(()); // keep-alive comments and the like
        };
        match self.flavor.parse_event(&value) {
            SseEvent::Ignore => {}
            SseEvent::Delta(delta) => {
                if let Delta::Text(text) = &delta {
                    reply.text.push_str(text);
                }
                let _ = deltas.send(delta);
            }
            SseEvent::Usage { input_tokens, output_tokens } => {
                reply.input_tokens = input_tokens;
                reply.output_tokens = output_tokens;
            }
            SseEvent::Failed(error) => return Err(error),
        }
        Ok(())
    }
}

impl LlmDriver for HttpCaller {
    fn call(&self, request: LlmRequest, deltas: mpsc::UnboundedSender<Delta>) -> DriverFuture {
        Box::pin(self.clone().run(request, deltas))
    }
}

// ---------------------------------------------------------------------------------------------
// Pure helpers (unit tested)
// ---------------------------------------------------------------------------------------------

/// Remove and return the first complete line (without `\r\n`), or `None` if there is none yet.
/// Works on bytes, so a UTF-8 character split across two network chunks stays intact.
pub fn take_line(buffer: &mut Vec<u8>) -> Option<String> {
    let end = buffer.iter().position(|&b| b == b'\n')?;
    let line: Vec<u8> = buffer.drain(..=end).collect();
    Some(String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']).to_string())
}

/// `data: {...}` → `{...}`; other SSE lines (`event:`, `:` comments, blank) → `None`.
pub fn sse_data(line: &str) -> Option<&str> {
    let data = line.strip_prefix("data:")?;
    Some(data.trim_start())
}

/// Chat messages in the `[{"role","content"}]` shape both OpenAI and OpenRouter use.
///
/// A message with images gets a list of parts instead of a string. The part shapes differ per provider, so
/// the flavor passes them in:
/// - OpenAI: `{"type":"input_text","text":…}` + `{"type":"input_image","image_url":"data:…"}`
/// - OpenRouter: `{"type":"text","text":…}` + `{"type":"image_url","image_url":{"url":"data:…"}}`
pub fn messages_json(messages: &[ChatMessage], parts: &ContentParts) -> Value {
    let mut list = Vec::new();
    for message in messages {
        let role = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        list.push(json!({ "role": role, "content": content_json(message, parts) }));
    }
    Value::Array(list)
}

/// How a provider spells a text part and an image part.
pub struct ContentParts {
    pub text_type: &'static str,
    /// Builds the image part from its `data:` URL.
    pub image_part: fn(String) -> Value,
}

fn content_json(message: &ChatMessage, parts: &ContentParts) -> Value {
    if message.images.is_empty() {
        return json!(message.content);
    }
    let mut list = Vec::new();
    for image in &message.images {
        list.push((parts.image_part)(data_url(image)));
    }
    list.push(json!({ "type": parts.text_type, "text": message.content }));
    Value::Array(list)
}

/// `data:image/png;base64,iVBOR…`
pub fn data_url(image: &Image) -> String {
    format!("data:{};base64,{}", image.mime_type, BASE64.encode(&image.data))
}

/// The request's JSON Schema as JSON (`LlmCaller` already checked that it parses).
pub fn parse_schema(schema: Option<&str>) -> Option<Value> {
    serde_json::from_str::<Value>(schema?).ok()
}

/// HTTP error status → domain error, with the provider's message when it sent one.
pub fn status_error(status: u16, body: &str) -> LlmError {
    let message = match serde_json::from_str::<Value>(body) {
        Ok(value) => value["error"]["message"].as_str().map(String::from).unwrap_or_else(|| excerpt(body, 300)),
        Err(_) => excerpt(body, 300),
    };
    let message = format!("HTTP {status}: {message}");
    match status {
        401 | 403 => LlmError::Unauthenticated(message),
        429 => LlmError::RateLimited(message),
        500.. => LlmError::Unavailable(message),
        _ => LlmError::Failed(message),
    }
}

#[cfg(test)]
#[path = "base_tests.rs"]
mod tests;
