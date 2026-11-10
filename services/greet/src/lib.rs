//! Greet service: says hello.
//!
//! - `domain.rs`  — the rules (pure functions, unit tested)
//! - `handler.rs` — proto ⇄ domain (implements the generated `GreetService` trait)
//! - `module()`   — the only thing the host needs

mod domain;
mod handler;

use std::sync::Arc;

use connectrpc::Router;
use kernel::{Deps, Module};

use handler::GreetHandler;

/// Create the module. `deps` is unused here; a service that calls others keeps
/// `deps.channel()` to build their generated clients.
pub fn module(_deps: &Deps) -> Arc<dyn Module> {
    Arc::new(GreetModule {
        handler: Arc::new(GreetHandler),
    })
}

struct GreetModule {
    handler: Arc<GreetHandler>,
}

impl Module for GreetModule {
    fn name(&self) -> &'static str {
        "greet"
    }

    fn register(&self, router: Router) -> Router {
        router.add_service(self.handler.clone())
    }
}
