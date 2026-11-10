//! Greet service: says hello. The reference shape every service copies:
//! `lib.rs` builds the module (the composition root), `handler.rs` holds all the logic.

mod handler;

use std::sync::Arc;

use kernel::{Deps, Module};

/// Called by the host once at startup (≈ registering the service in a C# DI container).
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    kernel::module("greet", handler::GreetHandler::new(deps))
}
