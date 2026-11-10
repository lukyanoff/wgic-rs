//! Unit tests for `channel.rs`.

use super::*;
use connectrpc::ErrorCode;

#[test]
fn connect_errors_pass_through_unchanged() {
    let original = ConnectError::new(ErrorCode::DeadlineExceeded, "too slow");
    let converted = to_connect_error(original);
    assert_eq!(converted.code, ErrorCode::DeadlineExceeded);
}

#[test]
fn other_errors_become_unavailable() {
    let io = std::io::Error::other("socket closed");
    let converted = to_connect_error(io);
    assert_eq!(converted.code, ErrorCode::Unavailable);
}
