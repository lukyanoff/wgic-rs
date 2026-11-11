//! The LLM drivers. Every driver streams: it reports `thinking` / `writing` steps and the answer text as it
//! arrives (`progress.text`), and returns the whole answer at the end. Dropping a call cancels it.
//!
//! ```text
//! Driver (interface)
//! ├── AgyDriver     `agy -p … --output-format stream-json`      (cli.rs runs the process)
//! ├── ClaudeDriver  `claude -p … --output-format stream-json`   (cli.rs runs the process)
//! └── HttpDriver    OpenAI-style chat completions over SSE (OpenAI, OpenRouter, …)
//! ```

pub mod agy;
pub mod claude;
pub mod cli;
pub mod http;

use crate::config::{Config, DriverKind};
use async_trait::async_trait;
use sdk::{ProgressSender, RpcError};
use std::sync::Arc;

// ============================================================================
// Inputs & outputs
// ============================================================================

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
    /// Context + user prompt (for drivers that take the system prompt separately).
    pub fn user_text(&self) -> String {
        let parts = [self.text_prompt.trim(), self.user_prompt.trim()];
        parts.iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join("\n\n")
    }

    /// System + context + user prompt as one text (for drivers without a system-prompt option).
    pub fn combined_prompt(&self) -> String {
        match self.system_prompt.trim() {
            "" => self.user_text(),
            system => format!("{system}\n\n{}", self.user_text()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmOutput {
    pub text: String,
    pub model: String,
}

// ============================================================================
// The interface (≈ C# `interface ILlmDriver`)
// ============================================================================

#[async_trait]
pub trait Driver: Send + Sync + 'static {
    async fn call(&self, args: PromptArgs, progress: ProgressSender) -> Result<LlmOutput, RpcError>;
}

// ============================================================================
// Factory (≈ a DI registration switch)
// ============================================================================

/// `model` empty = the CLI's own default model (then the reported model is just the driver name).
pub fn build_driver(cfg: Config) -> anyhow::Result<Arc<dyn Driver>> {
    let or = |value: String, fallback: &str| if value.is_empty() { fallback.to_string() } else { value };
    match cfg.kind {
        DriverKind::Agy => Ok(Arc::new(agy::AgyDriver::new(or(cfg.binary, "agy"), cfg.model))),
        DriverKind::Claude => Ok(Arc::new(claude::ClaudeDriver::new(or(cfg.binary, "claude"), cfg.model))),
        DriverKind::Http => {
            let endpoint = or(cfg.endpoint, "https://openrouter.ai/api/v1/chat/completions");
            Ok(Arc::new(http::HttpDriver::new(endpoint, cfg.api_key, or(cfg.model, "gpt-4o-mini"))))
        }
    }
}
