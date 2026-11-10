//! LLM caller service: sends a chat to an LLM and streams progress, then a JSON answer.
//!
//! One service "class", several running instances, each with its own injected driver
//! (`claude-cli`, `agy-cli`, `openai`, `openrouter`) and fixed model / effort / temperature.
//! - `config.rs`  — `LLM_CALLER_*` settings
//! - `domain.rs`  — `LlmDriver` (interface) + `LlmCaller` (the shared base-class body)
//! - `drivers/`   — `CliCaller` and `HttpCaller` base classes and their four flavors
//! - `handler.rs` — the server-streaming `Call` RPC
//! - `module()`   — the only thing the host needs

// ---------------------------------------------------------------------------------------------
// Modules and usings
// ---------------------------------------------------------------------------------------------

pub mod config;
pub mod domain;
pub mod drivers;
mod handler;
mod json;

use std::sync::Arc;

use connectrpc::Router;
use kernel::{Deps, Module};

use config::{LlmConfig, LlmSettings};
use domain::{LlmCaller, LlmDriver};
use drivers::unconfigured::Unconfigured;
use handler::LlmCallerHandler;

// ---------------------------------------------------------------------------------------------
// Composition root
// ---------------------------------------------------------------------------------------------

/// Create the module from env (`LLM_CALLER_DRIVER`, `_MODEL`, `_EFFORT`, `_TEMPERATURE`, ...).
/// A bad setting stops the host at startup with a clear message, rather than failing every call.
/// No `LLM_CALLER_DRIVER` at all: the host starts and every call answers `failed_precondition`.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    if deps.env(config::DRIVER_ENV).is_none() {
        return module_with(deps, Arc::new(Unconfigured), Arc::new(Unconfigured));
    }
    let settings = match LlmSettings::from_env(deps) {
        Ok(settings) => settings,
        Err(error) => panic!("llm_caller: {error}"),
    };
    let driver = drivers::create(deps, &settings);
    module_with(deps, Arc::new(settings), driver)
}

/// Create the module with an explicit config and driver (≈ registering your own `ILlmDriver`
/// in a C# DI container). Tests use it to inject a fake driver.
pub fn module_with(deps: &Deps, config: Arc<dyn LlmConfig>, driver: Arc<dyn LlmDriver>) -> Arc<dyn Module> {
    let handler = LlmCallerHandler::new(deps, LlmCaller::new(config, driver));
    Arc::new(LlmCallerModule { handler: Arc::new(handler) })
}

struct LlmCallerModule {
    handler: Arc<LlmCallerHandler>,
}

impl Module for LlmCallerModule {
    fn name(&self) -> &'static str {
        "llm_caller"
    }

    fn register(&self, router: Router) -> Router {
        router.add_service(self.handler.clone())
    }
}
