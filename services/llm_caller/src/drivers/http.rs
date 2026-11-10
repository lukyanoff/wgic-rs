use crate::drivers::{Driver, LlmOutput, PromptArgs};
use async_trait::async_trait;
use base64::Engine;
use sdk::RpcError;
use serde_json::{json, Value};

pub struct HttpDriver {
    endpoint: String,
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl HttpDriver {
    pub fn new(endpoint: String, api_key: String, model: String) -> Self {
        Self {
            endpoint,
            api_key,
            model,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Driver for HttpDriver {
    async fn call(&self, args: PromptArgs) -> Result<LlmOutput, RpcError> {
        let mut messages = Vec::new();

        if !args.system_prompt.trim().is_empty() {
            messages.push(json!({
                "role": "system",
                "content": args.system_prompt,
            }));
        }

        let mut user_content = Vec::new();
        let combined_user = if args.text_prompt.trim().is_empty() {
            args.user_prompt.clone()
        } else {
            format!("{}\n\n{}", args.text_prompt, args.user_prompt)
        };

        user_content.push(json!({
            "type": "text",
            "text": combined_user,
        }));

        for img in &args.images {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&img.data);
            let data_url = format!("data:{};base64,{}", img.mime_type, b64);
            user_content.push(json!({
                "type": "image_url",
                "image_url": { "url": data_url }
            }));
        }

        messages.push(json!({
            "role": "user",
            "content": user_content,
        }));

        let body = json!({
            "model": self.model,
            "messages": messages,
        });

        let mut req = self.client.post(&self.endpoint).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| RpcError::internal(format!("http llm request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(RpcError::internal(format!(
                "http llm returned {status}: {err_body}"
            )));
        }

        let payload: Value = resp
            .json()
            .await
            .map_err(|e| RpcError::internal(format!("http llm invalid json: {e}")))?;

        let text = payload["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let model = payload["model"]
            .as_str()
            .unwrap_or(&self.model)
            .to_string();

        Ok(LlmOutput { text, model })
    }
}
