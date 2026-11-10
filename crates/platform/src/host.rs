//! Run modules on the adapters chosen by the environment.

use std::sync::Arc;

use anyhow::{Context, bail};
use connectrpc::client::{ClientConfig, HttpClient};
use futures::future::try_join_all;
use kernel::{Channel, ChannelFactory, Deps, Module, ModuleFactory};

use crate::adapter::{Adapter, HttpAdapter, NatsAdapter};
use crate::addr_from_env;
use crate::nats::{NatsConfig, NatsTransport};
use crate::telemetry::{OtelMetrics, TracingLogs, TracingTransport};

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

    let runs = adapters.iter().map(|adapter| adapter.serve(modules.clone()));
    try_join_all(runs).await?;
    Ok(())
}

async fn adapters_from_env() -> anyhow::Result<Vec<Box<dyn Adapter>>> {
    let mut adapters: Vec<Box<dyn Adapter>> = Vec::new();
    let http_disabled = matches!(std::env::var("HTTP_DISABLED").as_deref(), Ok("1" | "true"));
    if !http_disabled {
        let addr = addr_from_env();
        let addr = addr.parse().with_context(|| format!("ADDR={addr} is not host:port"))?;
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

/// Build the services named in `wanted` (comma-separated, from `SERVICES`), and only those: a service
/// whose settings are missing never stops a host that doesn't run it. Unknown names list the known ones.
pub fn build_modules(all: &[(&'static str, ModuleFactory)], wanted: &str, deps: &Deps) -> anyhow::Result<Modules> {
    let known: Vec<&str> = all.iter().map(|(name, _)| *name).collect();
    let wanted: Vec<&str> = wanted.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if wanted.is_empty() {
        bail!("set SERVICES to a comma list of: {}", known.join(", "));
    }
    let mut modules = Vec::new();
    for name in wanted {
        let Some((_, build)) = all.iter().find(|(known, _)| *known == name) else {
            bail!("unknown service '{name}'; available: {}", known.join(", "));
        };
        modules.push(build(deps));
    }
    Ok(modules)
}

/// Deps for services: logs → `tracing` (JSON stdout + OTLP), metrics → OTel, and calls to
/// other services over NATS when `NATS_URL` is set, otherwise over HTTP to `UPSTREAM_ADDR`
/// (default: `ADDR`, i.e. this host itself).
pub async fn deps_from_env() -> anyhow::Result<Deps> {
    let deps = client_deps_from_env().await?;
    Ok(deps.with_logs(TracingLogs).with_metrics(OtelMetrics::new()))
}

async fn client_deps_from_env() -> anyhow::Result<Deps> {
    if let Some(config) = NatsConfig::from_env() {
        let nc = async_nats::connect(config.url.as_str())
            .await
            .with_context(|| format!("connect to NATS at {}", config.url))?;
        let channels = NatsChannels { nc };
        let channel = channels.channel_to(&config.prefix);
        // The base URI is unused by NatsTransport; only the RPC path becomes the subject.
        return Ok(Deps::new(channel, ClientConfig::new("http://nats".parse()?)).with_channels(channels));
    }
    let upstream = std::env::var("UPSTREAM_ADDR").unwrap_or_else(|_| addr_from_env());
    let channel = Channel::new(TracingTransport(HttpClient::plaintext()));
    let config = ClientConfig::new(format!("http://{upstream}").parse()?);
    Ok(Deps::new(channel, config))
}

/// `deps.channel_to("screen-1")` over NATS: the instance id is the subject prefix of the target host.
struct NatsChannels {
    nc: async_nats::Client,
}

impl ChannelFactory for NatsChannels {
    fn channel_to(&self, instance: &str) -> Channel {
        Channel::new(TracingTransport(NatsTransport::new(self.nc.clone(), instance)))
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
