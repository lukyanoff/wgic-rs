//! Visioner service: an orchestrator. Takes a screenshot from one sensor instance, asks several llm_caller
//! instances in parallel what they see, streams everyone's progress, and returns every answer.
//!
//! - `config.rs`  — `VISIONER_*` settings: which instances to call, timeouts
//! - `handler.rs` — the streaming `Check` RPC, calling the other instances with their generated clients

mod config;
mod handler;

use std::sync::Arc;

use kernel::{Deps, Module};

pub use config::VisionerSettings;
use handler::VisionerHandler;

/// Called by the host once at startup. Missing or bad settings stop the host.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    module_with(deps, &VisionerSettings::from_env(deps))
}

/// The module with explicit settings (tests point it at fake instances).
pub fn module_with(deps: &Deps, settings: &VisionerSettings) -> Arc<dyn Module> {
    kernel::module("visioner", VisionerHandler::new(deps, settings))
}
