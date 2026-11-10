//! Integration: the real generated client → full connect stack → GreetService, in-process.

use connectrpc::ErrorCode;
use contracts::proto::greet::v1::{GreetRequest, GreetServiceClient};
use kernel::{Channel, Deps};

fn client() -> GreetServiceClient<Channel> {
    let greet = greet::module(&Deps::in_process(&[]));
    let deps = Deps::in_process(&[greet]);
    GreetServiceClient::new(deps.channel(), deps.client_config())
}

fn request(name: &str) -> GreetRequest {
    GreetRequest {
        name: name.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn greets_by_name() {
    let response = client().greet(request("Jane")).await.unwrap();
    assert_eq!(response.view().greeting, "Hello, Jane!");
}

#[tokio::test]
async fn empty_name_is_invalid_argument() {
    let error = client().greet(request("")).await.err().unwrap();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert_eq!(error.message.as_deref(), Some("name must not be empty"));
}

#[tokio::test]
async fn too_long_name_is_invalid_argument() {
    let error = client()
        .greet(request(&"a".repeat(101)))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
}
