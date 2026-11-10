pub mod agy;
pub mod claude;
pub mod cli;
pub mod http;

use crate::config::{Config, DriverKind};
use async_trait::async_trait;
use sdk::RpcError;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ImageArg {
    pub data: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct PromptArgs {
    pub system_prompt: String,
    pub text_prompt: String,
    pub user_prompt: String,
    pub images: Vec<ImageArg>,
}

impl PromptArgs {
    pub fn combined_prompt(&self) -> String {
        let mut parts = Vec::new();
        if !self.system_prompt.trim().is_empty() {
            parts.push(format!("System:
{}", self.system_prompt.trim()));
        }
        if !self.text_prompt.trim().is_empty() {
            parts.push(format!("Context:
{}", self.text_prompt.trim()));
        }
        parts.push(self.user_prompt.trim().to_string());
        parts.join("

")
    }
}

#[derive(Debug, Clone)]
pub struct LlmOutput {
    pub text: String,
    pub model: String,
}

#[async_trait]
pub trait Driver: Send + Sync + 'static {
    async fn call(&self, args: PromptArgs) -> Result<LlmOutput, RpcError>;
}

pub fn build_driver(cfg: Config) -> anyhow::Result<Arc<dyn Driver>> {
    match cfg.kind {
        DriverKind::Agy => {
            let bin = if cfg.binary.is_empty() { "agy".into() } else { cfg.binary };
            let model = if cfg.model.is_empty() { "agy-default".into() } else { cfg.model };
            Ok(Arc::new(agy::AgyDriver::new(bin, model)))
        }
        DriverKind::Claude => {
            let bin = if cfg.binary.is_empty() { "claude".into() } else { cfg.binary };
            let model = if cfg.model.is_empty() { "claude-cli".into() } else { cfg.model };
            Ok(Arc::new(claude::ClaudeDriver::new(bin, model)))
        }
        DriverKind::Http => {
            let endpoint = if cfg.endpoint.is_empty() {
                "https://openrouter.ai/api/v1/chat/completions".into()
            } else {
                cfg.endpoint
            };
            let model = if cfg.model.is_empty() { "gpt-4o-mini".into() } else { cfg.model };
            Ok(Arc::new(http::HttpDriver::new(endpoint, cfg.api_key, model)))
        }
    }
}
