//! The four LLM callers, grouped by their base class:
//! - `cli/`  — `CliCaller` + flavors `ClaudeCli`, `AgyCli`
//! - `http/` — `HttpCaller` + flavors `OpenAi`, `OpenRouter`
//! - `unconfigured` — used when `LLM_CALLER_DRIVER` is not set
//!
//! `create` is the factory: settings in, the right driver out (≈ a C# DI registration switch).

pub mod cli {
    pub mod agy;
    pub mod base;
    pub mod claude;
}
pub mod http {
    pub mod base;
    pub mod openai;
    pub mod openrouter;
}
pub mod unconfigured;

use std::sync::Arc;

use kernel::Deps;

use crate::config::{DriverKind, LlmSettings};
use crate::domain::LlmDriver;
use cli::base::CliCaller;
use http::base::{HttpCaller, HttpFlavor};

/// Build the driver chosen by `settings.driver`. HTTP drivers read their API key here
/// (`OPENAI_API_KEY` / `OPENROUTER_API_KEY`); a missing key fails each call, not the startup.
pub fn create(deps: &Deps, settings: &LlmSettings) -> Arc<dyn LlmDriver> {
    match settings.driver {
        DriverKind::ClaudeCli => Arc::new(CliCaller::new(Arc::new(cli::claude::ClaudeCli), settings.clone())),
        DriverKind::AgyCli => Arc::new(CliCaller::new(Arc::new(cli::agy::AgyCli), settings.clone())),
        DriverKind::OpenAi => http_caller(deps, Arc::new(http::openai::OpenAi), settings),
        DriverKind::OpenRouter => http_caller(deps, Arc::new(http::openrouter::OpenRouter), settings),
    }
}

fn http_caller(deps: &Deps, flavor: Arc<dyn HttpFlavor>, settings: &LlmSettings) -> Arc<dyn LlmDriver> {
    let api_key = deps.env(flavor.api_key_env());
    Arc::new(HttpCaller::new(flavor, settings.clone(), api_key))
}
