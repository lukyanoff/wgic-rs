//! Greet service: says hello.
//!
//! - `domain.rs`  — the rules: `Greeter` interface + `HelloGreeter` implementation (unit tested)
//! - `handler.rs` — proto ⇄ domain + logs/metrics (implements the generated `GreetService` trait)
//! - `module()`   — the only thing the host needs

mod domain;
mod handler;

use std::sync::Arc;

use connectrpc::Router;
use kernel::{Deps, Module};

use domain::HelloGreeter;
use handler::GreetHandler;

/// Create the module: this service's composition root, where implementations are chosen
/// (≈ the `services.AddSingleton<IGreeter, Greeter>()` lines in C#).
/// A service that calls others also keeps `deps.channel()` to build their generated clients.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    let greeter = Arc::new(HelloGreeter::new());
    let handler = GreetHandler::new(deps, greeter);
    Arc::new(GreetModule { handler: Arc::new(handler) })
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
