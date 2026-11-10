//! Unit tests for `interceptor.rs`.

use super::*;

#[test]
fn protocol_names_follow_otel_conventions() {
    assert_eq!(protocol_names(Some(Protocol::Grpc)), ("grpc", "grpc"));
    assert_eq!(
        protocol_names(Some(Protocol::GrpcWeb)),
        ("grpc", "grpc_web")
    );
    assert_eq!(protocol_names(None), ("connect_rpc", "unknown"));
}

#[test]
fn finished_calls_are_counted_in_the_duration_histogram() {
    use crate::telemetry::test_meter::TestMeter;

    let test = TestMeter::new();
    let duration = test
        .meter()
        .f64_histogram("rpc.server.call.duration")
        .build();
    let call = Call {
        span: Span::none(),
        started: Instant::now(),
        attributes: vec![KeyValue::new("rpc.method", "Greet")],
        duration,
    };
    call.finish(None, None);
    call.finish(
        Some(&ConnectError::new(
            connectrpc::ErrorCode::InvalidArgument,
            "bad",
        )),
        None,
    );

    assert_eq!(test.totals()["rpc.server.call.duration"], 2.0);
}
