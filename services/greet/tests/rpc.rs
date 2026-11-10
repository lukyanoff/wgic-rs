//! Integration: the real generated client → full connect stack → GreetService, in-process.
//! Logs and metrics go to in-memory backends so tests can check them.

use connectrpc::ErrorCode;
use contracts::proto::greet::v1::{GreetRequest, GreetServiceClient};
use kernel::testing::{MemoryLogs, MemoryMetrics};
use kernel::{Channel, Deps, FieldValue, kv};

/// A client for a greet service whose logs and metrics are captured.
struct Setup {
    client: GreetServiceClient<Channel>,
    logs: MemoryLogs,
    metrics: MemoryMetrics,
}

fn setup() -> Setup {
    let logs = MemoryLogs::new();
    let metrics = MemoryMetrics::new();
    let service_deps = Deps::in_process(&[])
        .with_logs(logs.clone())
        .with_metrics(metrics.clone());
    let caller = Deps::in_process(&[greet::module(&service_deps)]);
    let client = GreetServiceClient::new(caller.channel(), caller.client_config());
    Setup {
        client,
        logs,
        metrics,
    }
}

fn request(name: &str) -> GreetRequest {
    GreetRequest {
        name: name.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn greets_by_name() {
    let response = setup().client.greet(request("Jane")).await.unwrap();
    assert_eq!(response.view().greeting, "Hello, Jane!");
}

#[tokio::test]
async fn logs_and_counts_each_greeting() {
    let s = setup();
    s.client.greet(request("Jane")).await.unwrap();

    let entry = s.logs.find("greeting sent").unwrap();
    assert_eq!(entry.field("name"), Some(&FieldValue::Str("Jane".into())));
    assert_eq!(s.metrics.sum("greet.greetings"), 1.0);
    assert_eq!(s.metrics.points()[0].tags, [kv("transport", "http")]);
}

#[tokio::test]
async fn empty_name_is_invalid_argument() {
    let error = setup().client.greet(request("")).await.err().unwrap();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert_eq!(error.message.as_deref(), Some("name must not be empty"));
}

#[tokio::test]
async fn rejections_are_logged_and_counted_by_reason() {
    let s = setup();
    s.client.greet(request("")).await.err().unwrap();

    let entry = s.logs.find("greeting rejected").unwrap();
    assert_eq!(
        entry.field("reason"),
        Some(&FieldValue::Str("empty_name".into()))
    );
    assert_eq!(s.metrics.sum("greet.rejected"), 1.0);
    assert_eq!(s.metrics.sum("greet.greetings"), 0.0);
}

#[tokio::test]
async fn too_long_name_is_invalid_argument() {
    let error = setup()
        .client
        .greet(request(&"a".repeat(101)))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
}
