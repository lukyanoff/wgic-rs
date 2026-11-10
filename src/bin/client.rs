use connect_rust_example::addr_from_env;
use connect_rust_example::proto::greet::v1::{GreetRequest, GreetServiceClient};
use connectrpc::client::{ClientConfig, HttpClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = GreetServiceClient::new(
        HttpClient::plaintext(),
        ClientConfig::new(format!("http://{}", addr_from_env()).parse()?),
    );
    let res = client
        .greet(GreetRequest {
            name: "Jane".into(),
            ..Default::default()
        })
        .await?;
    println!("{}", res.view().greeting);
    Ok(())
}
