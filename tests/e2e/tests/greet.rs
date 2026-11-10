//! E2E: GreetService in the real `host` binary, over HTTP and over NATS.

use connectrpc::ErrorCode;
use contracts::proto::greet::v1::{GreetRequest, GreetServiceClient};
use e2e::Host;
use kernel::{Channel, Deps};

fn client(deps: &Deps) -> GreetServiceClient<Channel> {
    GreetServiceClient::new(deps.channel(), deps.client_config())
}

fn request(name: &str) -> GreetRequest {
    GreetRequest {
        name: name.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn greets_over_http() {
    let host = Host::start("greet").await;
    let response = client(&host.http_deps())
        .greet(request("Jane"))
        .await
        .unwrap();
    assert_eq!(response.view().greeting, "Hello, Jane!");
}

#[tokio::test]
async fn greets_over_nats() {
    let host = Host::start("greet").await;
    let deps = host.nats_deps().await;
    let response = client(&deps).greet(request("Jane")).await.unwrap();
    assert_eq!(response.view().greeting, "Hello, Jane!");
}

#[tokio::test]
async fn invalid_argument_survives_nats_round_trip() {
    let host = Host::start("greet").await;
    let deps = host.nats_deps().await;
    let error = client(&deps).greet(request("")).await.err().unwrap();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert_eq!(error.message.as_deref(), Some("name must not be empty"));
}
