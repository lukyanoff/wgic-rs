//! OpenAI-style chat completions with `"stream": true` (OpenAI, OpenRouter, …). The answer arrives as
//! server-sent events: `data: {"choices":[{"delta":{"content":"…"}}]}` lines, ending with `data: [DONE]`.

use crate::drivers::{Driver, LlmOutput, PromptArgs};
use async_trait::async_trait;
use base64::Engine;
use futures::StreamExt;
use sdk::{ProgressSender, RpcError};
use serde_json::{Value, json};

pub struct HttpDriver {
    endpoint: String,
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl HttpDriver {
    pub fn new(endpoint: String, api_key: String, model: String) -> Self {
        Self { endpoint, api_key, model, client: reqwest::Client::new() }
    }

    fn body(&self, args: &PromptArgs) -> Value {
        let mut messages = Vec::new();
        if !args.system_prompt.trim().is_empty() {
            messages.push(json!({"role": "system", "content": args.system_prompt}));
        }
        let mut content = vec![json!({"type": "text", "text": args.user_text()})];
        for img in &args.images {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&img.data);
            content.push(
                json!({"type": "image_url", "image_url": {"url": format!("data:{};base64,{b64}", img.mime_type)}}),
            );
        }
        messages.push(json!({"role": "user", "content": content}));
        json!({"model": self.model, "messages": messages, "stream": true})
    }
}

#[async_trait]
impl Driver for HttpDriver {
    async fn call(&self, args: PromptArgs, progress: ProgressSender) -> Result<LlmOutput, RpcError> {
        let mut req = self.client.post(&self.endpoint).json(&self.body(&args));
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req.send().await.map_err(|e| RpcError::unavailable(format!("http llm request failed: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(RpcError::internal(format!("http llm returned {status}: {err_body}")));
        }

        progress.report("writing");
        let mut events = SseText::default();
        let mut chunks = resp.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|e| RpcError::unavailable(format!("http llm stream broke: {e}")))?;
            for delta in events.push(&chunk) {
                progress.text(delta);
            }
        }
        let model = events.model.unwrap_or_else(|| self.model.clone());
        Ok(LlmOutput { text: events.text.trim().to_string(), model })
    }
}

// ============================================================================
// SSE parsing (pure, unit tested)
// ============================================================================

/// Collects the answer from SSE bytes that may split anywhere, even inside a line.
#[derive(Default)]
pub struct SseText {
    partial_line: String,
    pub text: String,
    pub model: Option<String>,
}

impl SseText {
    /// Feed one network chunk; returns the new answer pieces it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.partial_line.push_str(&String::from_utf8_lossy(chunk));
        let mut deltas = Vec::new();
        while let Some(end) = self.partial_line.find('\n') {
            let line: String = self.partial_line.drain(..=end).collect();
            if let Some(delta) = self.on_line(line.trim()) {
                self.text.push_str(&delta);
                deltas.push(delta);
            }
        }
        deltas
    }

    fn on_line(&mut self, line: &str) -> Option<String> {
        let data = line.strip_prefix("data:")?.trim();
        let event: Value = serde_json::from_str(data).ok()?; // `[DONE]` and comments are not JSON
        if self.model.is_none() {
            self.model = event["model"].as_str().map(String::from);
        }
        let delta = event["choices"][0]["delta"]["content"].as_str()?;
        (!delta.is_empty()).then(|| delta.to_string())
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod http_tests;
