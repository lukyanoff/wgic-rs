//! Unit tests for `transport.rs`.

use super::*;

#[test]
fn names() {
    assert_eq!(Transport::Http.name(), "http");
    let nats = Transport::Nats {
        subject: "rpc.x".into(),
    };
    assert_eq!(nats.name(), "nats");
}
