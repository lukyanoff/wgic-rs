//! LLM caller service: sends a chat to an LLM, streams progress, then returns a JSON answer.
//! One service, several running instances, each with its own driver and model (from its settings).
//!
//! - `config.rs`  — `LLM_CALLER_*` settings
//! - `drivers.rs` — `LlmDriver` and its implementations (claude, agy, OpenAI/OpenRouter)
//! - `handler.rs` — the streaming `Call` RPC: check the chat, run the driver, stream progress, extract JSON

mod config;
pub mod drivers;
mod handler;

use std::sync::Arc;

use kernel::{Deps, Module};

pub use config::LlmSettings;
use drivers::LlmDriver;
use handler::LlmCallerHandler;

/// Called by the host once at startup. Missing or bad settings stop the host.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    let settings = LlmSettings::from_env(deps);
    let driver = drivers::create(deps, &settings);
    module_with(deps, settings, driver)
}

/// The module with an explicit driver (tests pass a fake one).
pub fn module_with(deps: &Deps, settings: LlmSettings, driver: Arc<dyn LlmDriver>) -> Arc<dyn Module> {
    kernel::module("llm_caller", LlmCallerHandler::new(deps, settings, driver))
}
