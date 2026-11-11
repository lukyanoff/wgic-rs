//! Box 4: the composition root (≈ `Program.cs`). Parses the CLI, builds drivers/registries/state stores,
//! injects them into the service handlers, and serves the result as a NATS micro service.

use clap::Parser;
use contracts::{LlmCallerEndpoint, SensorEndpoint, VisionerEndpoint};
use sdk::state::NatsKvStateStore;
use sdk::{ILogger, NatsLlmRegistry, NatsSensorRegistry, OtelLogger, RawRpcEndpoint};
use std::sync::Arc;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    module: String,
    #[arg(long)]
    instance: String,
    #[arg(long, default_value = "{}")]
    config: String,
    #[arg(long, env = "WGIC_NATS_URL", default_value = "nats://127.0.0.1:4222")]
    nats_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    // Kept until `main` returns: dropping it flushes the last spans, logs and metrics.
    let _telemetry = sdk::telemetry::init_telemetry(&args.module, &args.instance);

    let nc = async_nats::ConnectOptions::new()
        .name(format!("{}/{}", args.module, args.instance))
        .connect(&args.nats_url)
        .await?;
    let js = async_nats::jetstream::new(nc.clone());
    let kv = js
        .create_key_value(async_nats::jetstream::kv::Config {
            bucket: "wgic_instance_state".into(),
            history: 1,
            ..Default::default()
        })
        .await?;
    let state_key = format!("{}.{}", args.module, args.instance);
    // One logger per process; its entries carry service, instance and trace context automatically.
    let log: Arc<dyn ILogger> = Arc::new(OtelLogger::new(&args.module));

    let endpoint: Arc<dyn RawRpcEndpoint> = match args.module.as_str() {
        "sensor" => {
            let cfg: sensor::Config = serde_json::from_str(&args.config)?;
            let driver = sensor::build_driver(&cfg);
            let state = Arc::new(NatsKvStateStore::new(kv, state_key));
            Arc::new(SensorEndpoint(Arc::new(sensor::SensorHandler::new(driver, state, log))))
        }
        "llm_caller" => {
            let cfg: llm_caller::Config = serde_json::from_str(&args.config)?;
            let driver = llm_caller::build_driver(cfg)?;
            let state = Arc::new(NatsKvStateStore::new(kv, state_key));
            Arc::new(LlmCallerEndpoint(Arc::new(llm_caller::LlmCallerHandler::new(driver, state, log))))
        }
        "visioner" => {
            let cfg: visioner::Config = serde_json::from_str(&args.config)?;
            let state = Arc::new(NatsKvStateStore::new(kv, state_key));
            let clients = visioner::VisionerClients {
                sensors: Arc::new(NatsSensorRegistry::new(nc.clone(), cfg.sensor_timeout())),
                llms: Arc::new(NatsLlmRegistry::new(nc.clone(), cfg.llm_timeout())),
            };
            Arc::new(VisionerEndpoint(Arc::new(visioner::VisionerHandler::new(cfg, state, clients, log))))
        }
        other => anyhow::bail!("unknown module: {other}"),
    };

    let _handles = sdk::serve_endpoint(nc, &args.module, &args.instance, endpoint).await?;
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    Ok(())
}
