//! Run modules on the adapters chosen by the environment.

use std::sync::Arc;

use anyhow::{Context, bail};
use connectrpc::client::{ClientConfig, HttpClient};
use futures::future::try_join_all;
use kernel::{Channel, Deps, Module};

use crate::adapter::{Adapter, HttpAdapter, NatsAdapter};
use crate::addr_from_env;
use crate::nats::{NatsConfig, NatsTransport};
use crate::telemetry::TracingTransport;

/// The services a host runs, shared by every adapter.
pub type Modules = Vec<Arc<dyn Module>>;

/// Serve `modules` on every enabled adapter until one stops:
/// - HTTP on `ADDR`, unless `HTTP_DISABLED=1`
/// - NATS micro if `NATS_URL` is set (see [`NatsConfig::from_env`])
pub async fn run(modules: Modules) -> anyhow::Result<()> {
    let adapters = adapters_from_env().await?;
    if adapters.is_empty() {
        bail!("nothing to serve: HTTP_DISABLED is set and NATS_URL is not");
    }
    let services: Vec<&str> = modules.iter().map(|m| m.name()).collect();
    let transports: Vec<&str> = adapters.iter().map(|a| a.name()).collect();
    tracing::info!(?services, ?transports, "host: starting");

    let runs = adapters
        .iter()
        .map(|adapter| adapter.serve(modules.clone()));
    try_join_all(runs).await?;
    Ok(())
}

async fn adapters_from_env() -> anyhow::Result<Vec<Box<dyn Adapter>>> {
    let mut adapters: Vec<Box<dyn Adapter>> = Vec::new();
    let http_disabled = matches!(std::env::var("HTTP_DISABLED").as_deref(), Ok("1" | "true"));
    if !http_disabled {
        let addr = addr_from_env();
        let addr = addr
            .parse()
            .with_context(|| format!("ADDR={addr} is not host:port"))?;
        adapters.push(Box::new(HttpAdapter { addr }));
    }
    if let Some(config) = NatsConfig::from_env() {
        let client = async_nats::ConnectOptions::new()
            .name("host")
            .connect(config.url.as_str())
            .await
            .with_context(|| format!("connect to NATS at {}", config.url))?;
        adapters.push(Box::new(NatsAdapter { client, config }));
    }
    Ok(adapters)
}

/// Keep the modules named in `wanted` (comma-separated, e.g. from `SERVICES`).
/// Empty or `all` keeps every module. Unknown names are an error that lists the known ones.
pub fn select_modules(all: Modules, wanted: &str) -> anyhow::Result<Modules> {
    let wanted: Vec<&str> = wanted
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if wanted.is_empty() || wanted == ["all"] {
        return Ok(all);
    }
    let known: Vec<&str> = all.iter().map(|m| m.name()).collect();
    if let Some(unknown) = wanted.iter().find(|name| !known.contains(name)) {
        bail!(
            "unknown service '{unknown}'; available: {}",
            known.join(", ")
        );
    }
    Ok(all
        .into_iter()
        .filter(|m| wanted.contains(&m.name()))
        .collect())
}

/// Deps for calling other services: over NATS when `NATS_URL` is set, otherwise
/// over HTTP to `UPSTREAM_ADDR` (default: `ADDR`, i.e. this host itself).
pub async fn deps_from_env() -> anyhow::Result<Deps> {
    if let Some(config) = NatsConfig::from_env() {
        let nc = async_nats::connect(config.url.as_str())
            .await
            .with_context(|| format!("connect to NATS at {}", config.url))?;
        let channel = Channel::new(TracingTransport(NatsTransport::new(nc, config.prefix)));
        // The base URI is unused by NatsTransport; only the RPC path becomes the subject.
        return Ok(Deps::new(
            channel,
            ClientConfig::new("http://nats".parse()?),
        ));
    }
    let upstream = std::env::var("UPSTREAM_ADDR").unwrap_or_else(|_| addr_from_env());
    let channel = Channel::new(TracingTransport(HttpClient::plaintext()));
    let config = ClientConfig::new(format!("http://{upstream}").parse()?);
    Ok(Deps::new(channel, config))
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
