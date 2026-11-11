use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry(service_name: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ =
        tracing_subscriber::registry().with(filter).with(tracing_subscriber::fmt::layer().with_target(true)).try_init();
    tracing::info!(service = %service_name, "telemetry initialized");
}
