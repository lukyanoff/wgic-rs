use connect_rust_example::proto::greet::v1::{GreetRequest, GreetServiceClient};
use connect_rust_example::{NatsTransport, addr_from_env};
use connectrpc::client::{ClientConfig, ClientTransport, HttpClient};

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
    println!("{}", res.view().greeting);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    // NATS_URL set: call over NATS micro; otherwise over HTTP at ADDR.
    if let Ok(url) = std::env::var("NATS_URL") {
        let prefix = std::env::var("NATS_PREFIX").unwrap_or_else(|_| "rpc".into());
        let nc = async_nats::connect(url).await?;
        let transport = NatsTransport::new(nc, prefix);
        // The base URI is unused by NatsTransport; only the RPC path is mapped to a subject.
        greet(GreetServiceClient::new(
            transport,
            ClientConfig::new("http://nats".parse()?),
        ))
        .await
    } else {
        greet(GreetServiceClient::new(
            HttpClient::plaintext(),
            ClientConfig::new(format!("http://{}", addr_from_env()).parse()?),
        ))
        .await
    }
}
