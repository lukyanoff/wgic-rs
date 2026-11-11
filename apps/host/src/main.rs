use clap::Parser;
use contracts::{LlmCallerEndpoint, OkfReaderEndpoint, OkfWriterEndpoint, SensorEndpoint, VisionerEndpoint};
use sdk::state::NatsKvStateStore;
use sdk::{NatsLlmRegistry, NatsSensorRegistry, RawRpcEndpoint};
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
    let service_id = format!("{}/{}", args.module, args.instance);
    sdk::telemetry::init_telemetry(&service_id);

    let nc = async_nats::connect(&args.nats_url).await?;
    let js = async_nats::jetstream::new(nc.clone());

    let kv = js
        .create_key_value(async_nats::jetstream::kv::Config {
            bucket: "wgic_instance_state".into(),
            history: 1,
            ..Default::default()
        })
        .await?;

    let endpoint = build_endpoint(&args, nc.clone(), kv)?;

    let _handles = sdk::transport::serve_endpoint(nc, &args.module, &args.instance, endpoint).await?;
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down {}", service_id);
    Ok(())
}

/// Builds the one service this process runs (`--module`), with its dependencies injected.
fn build_endpoint(
    args: &Args,
    nc: async_nats::Client,
    kv: async_nats::jetstream::kv::Store,
) -> anyhow::Result<Arc<dyn RawRpcEndpoint>> {
    let state_key = format!("{}.{}", args.module, args.instance);

    let endpoint: Arc<dyn RawRpcEndpoint> = match args.module.as_str() {
        "sensor" => {
            let cfg: sensor::Config = serde_json::from_str(&args.config)?;
            let driver = sensor::build_driver(&cfg);
            let state = Arc::new(NatsKvStateStore::new(kv, state_key));
            Arc::new(SensorEndpoint(Arc::new(sensor::SensorHandler::new(driver, state))))
        }
        "llm_caller" => {
            let cfg: llm_caller::Config = serde_json::from_str(&args.config)?;
            let driver = llm_caller::build_driver(cfg)?;
            let state = Arc::new(NatsKvStateStore::new(kv, state_key));
            Arc::new(LlmCallerEndpoint(Arc::new(llm_caller::LlmCallerHandler::new(driver, state))))
        }
        "visioner" => {
            let cfg: visioner::Config = serde_json::from_str(&args.config)?;
            let state = Arc::new(NatsKvStateStore::new(kv, state_key));
            let sensors = Arc::new(NatsSensorRegistry::new(nc.clone()));
            let llms = Arc::new(NatsLlmRegistry::new(nc.clone()));
            Arc::new(VisionerEndpoint(Arc::new(visioner::VisionerHandler::new(
                cfg, state, sensors, llms,
            ))))
        }
        "okf_writer" => {
            let cfg: okf::Config = serde_json::from_str(&args.config)?;
            let store = Arc::new(okf::FileStore::new(cfg.root));
            let state = Arc::new(NatsKvStateStore::new(kv, state_key));
            Arc::new(OkfWriterEndpoint(Arc::new(okf::OkfWriterHandler::new(store, state))))
        }
        "okf_reader" => {
            let cfg: okf::Config = serde_json::from_str(&args.config)?;
            let store = Arc::new(okf::FileStore::new(cfg.root));
            let state = Arc::new(NatsKvStateStore::new(kv, state_key));
            Arc::new(OkfReaderEndpoint(Arc::new(okf::OkfReaderHandler::new(store, state))))
        }
        other => anyhow::bail!("unknown module: {other}"),
    };
    Ok(endpoint)
}
