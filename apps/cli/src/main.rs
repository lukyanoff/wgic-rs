//! `cargo run -p cli [name]` → prints "Hello, <name>!".
//! Calls over NATS when `NATS_URL` is set, otherwise over HTTP to `ADDR`.

use contracts::proto::greet::v1::{GreetRequest, GreetServiceClient};
use tracing::Instrument;

async fn call(name: String) -> anyhow::Result<String> {
    let deps = platform::deps_from_env().await?;
    let client = GreetServiceClient::new(deps.channel(), deps.client_config());
    let request = GreetRequest {
        name,
        ..Default::default()
    };
    let response = client.greet(request).await?;
    Ok(response.view().greeting.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = platform::init_telemetry("cli");
    let name = std::env::args().nth(1).unwrap_or_else(|| "Jane".into());
    let span = tracing::info_span!("cli.greet", otel.kind = "internal");
    let result = call(name).instrument(span.clone()).await;
    match result {
        Ok(greeting) => {
            span.in_scope(|| tracing::info!(%greeting, "reply"));
            println!("{greeting}");
            Ok(())
        }
        Err(e) => {
            span.in_scope(|| tracing::error!(error = %e, "call failed"));
            Err(e)
        }
    }
}
