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
