//! Runs the services named in `SERVICES=greet,sensor` (required). Only those are built, so each one reads
//! (and checks) only its own settings. Transports come from the environment: `ADDR`, `HTTP_DISABLED`,
//! `NATS_URL` (see HUMAN.md).

use kernel::ModuleFactory;

/// Every service this host knows: its name and constructor. Add one line per new service.
const ALL_SERVICES: [(&str, ModuleFactory); 4] = [
    ("greet", greet::module),
    ("sensor", sensor::module),
    ("llm_caller", llm_caller::module),
    ("visioner", visioner::module),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = platform::init_telemetry("host");
    let deps = platform::deps_from_env().await?;
    let wanted = std::env::var("SERVICES").unwrap_or_default();
    let modules = platform::build_modules(&ALL_SERVICES, &wanted, &deps)?;
    if let Err(e) = platform::run(modules).await {
        tracing::error!(error = %e, "host stopped");
        return Err(e);
    }
    Ok(())
}
