use connect_rust_example::proto::greet::v1::{GreetRequest, GreetServiceClient};
use connect_rust_example::{NatsTransport, TracingTransport, addr_from_env, init_telemetry};
use connectrpc::client::{ClientConfig, ClientTransport, HttpClient};
use tracing::Instrument;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Same generated client, any transport.
async fn greet<T>(client: GreetServiceClient<T>) -> Result<(), BoxError>
where
    T: ClientTransport,
    <T::ResponseBody as http_body::Body>::Error: std::fmt::Display,
{
    let res = client
        .greet(GreetRequest {
            name: "Jane".into(),
            ..Default::default()
        })
        .await?;
    let greeting = res.view().greeting.to_string();
    tracing::info!(%greeting, "reply");
    println!("{greeting}");
    Ok(())
}

async fn call() -> Result<(), BoxError> {
    // NATS_URL set: call over NATS micro; otherwise over HTTP at ADDR.
    if let Ok(url) = std::env::var("NATS_URL") {
        let prefix = std::env::var("NATS_PREFIX").unwrap_or_else(|_| "rpc".into());
        let nc = async_nats::connect(url).await?;
        let transport = TracingTransport(NatsTransport::new(nc, prefix));
        // The base URI is unused by NatsTransport; only the RPC path is mapped to a subject.
        greet(GreetServiceClient::new(
            transport,
            ClientConfig::new("http://nats".parse()?),
        ))
        .await
    } else {
        greet(GreetServiceClient::new(
            TracingTransport(HttpClient::plaintext()),
            ClientConfig::new(format!("http://{}", addr_from_env()).parse()?),
        ))
        .await
    }
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _telemetry = init_telemetry("greet-client");
    let span = tracing::info_span!("client.greet", otel.kind = "internal");
    let res = call().instrument(span.clone()).await;
    if let Err(e) = &res {
        span.in_scope(|| tracing::error!(error = %e, "call failed"));
    }
    res
}
