//! Runs any set of services: `SERVICES=greet,billing` (default: all).
//! Transports come from the environment: `ADDR`, `HTTP_DISABLED`, `NATS_URL` (see HUMAN.md).

use std::sync::Arc;

use kernel::{Deps, Module};

/// Every service this host knows. Add one line per new service.
fn all_modules(deps: &Deps) -> Vec<Arc<dyn Module>> {
    vec![greet::module(deps)]
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = platform::init_telemetry("host");
    let deps = platform::deps_from_env().await?;
    let wanted = std::env::var("SERVICES").unwrap_or_default();
    let modules = platform::select_modules(all_modules(&deps), &wanted)?;
    if let Err(e) = platform::run(modules).await {
        tracing::error!(error = %e, "host stopped");
        return Err(e);
    }
    Ok(())
}
