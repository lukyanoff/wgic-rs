//! Unit tests for `client.rs`.

use super::*;

#[test]
fn timeout_comes_from_connect_header() {
    let mut headers = http::HeaderMap::new();
    assert_eq!(timeout_from(&headers), None);
    headers.insert("connect-timeout-ms", HeaderValue::from_static("1500"));
    assert_eq!(timeout_from(&headers), Some(Duration::from_millis(1500)));
}

#[test]
fn error_reply_becomes_connect_error_json() {
    let mut headers = HeaderMap::new();
    headers.insert(CONNECT_CODE, "not_found");
    headers.insert(NATS_SERVICE_ERROR, "no such user");
    let response = error_response(&headers, "404");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn missing_connect_code_is_derived_from_status() {
    let response = error_response(&HeaderMap::new(), "503");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
