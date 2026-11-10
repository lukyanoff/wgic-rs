//! The LLM drivers, a small class hierarchy (Rust has no inheritance, so a "base class" is a struct that holds
//! an interface for the parts that differ):
//!
//! ```text
//! LlmDriver (interface)
//! ├── CliDriver  (base: start the program, feed it, read one JSON line at a time)  + CliFlavor (interface)
//! │   ├── ClaudeCli   `claude -p`
//! │   └── AgyCli      `agy -p`
//! └── HttpDriver (OpenAI chat-completions over SSE: OpenAI and OpenRouter)
//! ```

mod agy;
mod claude;
mod cli;
mod http;

use std::sync::Arc;

use connectrpc::ConnectError;
use futures::future::BoxFuture;
use kernel::Deps;
use tokio::sync::mpsc;

use crate::config::LlmSettings;

pub use agy::AgyCli;
pub use claude::ClaudeCli;
pub use cli::{CliDriver, CliFlavor, LineEvent};
pub use contracts::proto::llm_caller::v1::{CallRequest, ChatMessage, Image};
pub use http::HttpDriver;

// ---------------------------------------------------------------------------------------------
// The interface
// ---------------------------------------------------------------------------------------------

/// ≈ C# `interface ILlmDriver`.
pub trait LlmDriver: Send + Sync {
    /// Send the chat (its first message is already the JSON instruction); report output pieces on `deltas`
    /// while they arrive; return the whole answer. Dropping the returned future cancels the call.
    fn call(&self, request: CallRequest, deltas: Deltas) -> DriverFuture;
}

/// Where a driver reports output pieces (≈ C# `ChannelWriter<Delta>`).
pub type Deltas = mpsc::UnboundedSender<Delta>;

/// The async result of a driver call (≈ C# `Task<LlmReply>`), boxed so `LlmDriver` can be an interface.
pub type DriverFuture = BoxFuture<'static, Result<LlmReply, ConnectError>>;

/// One piece of streamed output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    /// The model is reasoning (that text is not part of the answer).
    Thinking,
    /// New answer text.
    Text(String),
}

/// What a driver returns when the model is done.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LlmReply {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ---------------------------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------------------------

/// The driver named by `settings.driver` (≈ a DI registration switch). An unknown name stops the host.
pub fn create(deps: &Deps, settings: &LlmSettings) -> Arc<dyn LlmDriver> {
    match settings.driver.as_str() {
        "claude-cli" => Arc::new(CliDriver::new(ClaudeCli, settings)),
        "agy-cli" => Arc::new(CliDriver::new(AgyCli, settings)),
        "openai" | "openrouter" => Arc::new(HttpDriver::new(deps, settings)),
        other => panic!("LLM_CALLER_DRIVER={other:?} is invalid, expected claude-cli, agy-cli, openai or openrouter"),
    }
}
