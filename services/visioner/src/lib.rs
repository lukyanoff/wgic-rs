//! Visioner service: an orchestrator. Takes a screenshot from one screen sensor instance, asks K llm_caller
//! instances in parallel what they see, streams everyone's progress, and returns every answer.
//!
//! - `config.rs`  — `VISIONER_*` settings: which instances to call, timeouts
//! - `domain.rs`  — `Screen` + `VisionLlm` (interfaces) and `Visioner` (the orchestration rules)
//! - `clients.rs` — those interfaces over the generated clients (from the `libs/clients` factories)
//! - `handler.rs` — the server-streaming `Check` RPC
//! - `module()`   — the only thing the host needs

// ---------------------------------------------------------------------------------------------
// Modules and usings
// ---------------------------------------------------------------------------------------------

pub mod clients;
pub mod config;
pub mod domain;
mod handler;

use std::sync::Arc;

use ::clients::{LlmCallerClients, ScreenSensorClients};
use connectrpc::Router;
use kernel::{Deps, Module};

use clients::{LlmClient, ScreenClient};
use config::VisionerSettings;
use domain::{Screen, VisionLlm, Visioner};
use handler::VisionerHandler;

// ---------------------------------------------------------------------------------------------
// Composition root
// ---------------------------------------------------------------------------------------------

/// Create the module from env. Without `VISIONER_SCREEN` (a host running other services) or without
/// NATS, every call answers `failed_precondition`. A bad setting stops the host at startup.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    if deps.env(config::SCREEN_ENV).is_none() {
        let message = format!("set {} and {} for this host", config::SCREEN_ENV, config::LLMS_ENV);
        return not_configured(deps, message);
    }
    if deps.env("NATS_URL").is_none() {
        return not_configured(deps, "visioner calls other instances over NATS: set NATS_URL".into());
    }
    let settings = match VisionerSettings::from_env(deps) {
        Ok(settings) => settings,
        Err(error) => panic!("visioner: {error}"),
    };
    let screen = ScreenClient::new(&ScreenSensorClients::new(deps), &settings.screen, settings.screen_timeout);
    let llm_factory = LlmCallerClients::new(deps);
    let mut llms: Vec<Arc<dyn VisionLlm>> = Vec::new();
    for id in &settings.llms {
        llms.push(Arc::new(LlmClient::new(&llm_factory, id, settings.llm_timeout)));
    }
    module_with(deps, &settings, Arc::new(screen), llms)
}

/// Create the module with an explicit screen and models (≈ registering your own `IScreen` in a C# DI
/// container). Tests use it with fakes or with clients to in-process instances.
pub fn module_with(
    deps: &Deps,
    settings: &VisionerSettings,
    screen: Arc<dyn Screen>,
    llms: Vec<Arc<dyn VisionLlm>>,
) -> Arc<dyn Module> {
    let visioner = Visioner::new(screen, llms, (settings.screen_timeout, settings.llm_timeout));
    let handler = VisionerHandler::new(deps, &settings.id, Ok(visioner));
    Arc::new(VisionerModule { handler: Arc::new(handler) })
}

fn not_configured(deps: &Deps, message: String) -> Arc<dyn Module> {
    let handler = VisionerHandler::new(deps, "vision", Err(message));
    Arc::new(VisionerModule { handler: Arc::new(handler) })
}

struct VisionerModule {
    handler: Arc<VisionerHandler>,
}

impl Module for VisionerModule {
    fn name(&self) -> &'static str {
        "visioner"
    }

    fn register(&self, router: Router) -> Router {
        router.add_service(self.handler.clone())
    }
}
