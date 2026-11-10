//! Unit tests for `base.rs`: SSE line handling and HTTP status mapping.

use super::*;

#[test]
fn lines_are_taken_one_by_one() {
    let mut buffer = b"data: a\r\n\ndata: b".to_vec();
    assert_eq!(take_line(&mut buffer).as_deref(), Some("data: a"));
    assert_eq!(take_line(&mut buffer).as_deref(), Some(""));
    assert_eq!(take_line(&mut buffer), None); // "data: b" is not complete yet
    assert_eq!(buffer, b"data: b");
}

#[test]
fn utf8_split_across_chunks_stays_intact() {
    let bytes = "data: é\n".as_bytes();
    let mut buffer = bytes[..7].to_vec(); // half of 'é'
    assert_eq!(take_line(&mut buffer), None);
    buffer.extend_from_slice(&bytes[7..]);
    assert_eq!(take_line(&mut buffer).as_deref(), Some("data: é"));
}

#[test]
fn only_data_lines_carry_events() {
    assert_eq!(sse_data("data: {\"a\":1}"), Some("{\"a\":1}"));
    assert_eq!(sse_data("data:[DONE]"), Some("[DONE]"));
    assert_eq!(sse_data("event: response.created"), None);
    assert_eq!(sse_data(": keep-alive"), None);
}

#[test]
fn status_codes_map_to_errors() {
    let body = r#"{"error":{"message":"Incorrect API key"}}"#;
    assert_eq!(status_error(401, body), LlmError::Unauthenticated("HTTP 401: Incorrect API key".into()));
    assert!(matches!(status_error(429, ""), LlmError::RateLimited(_)));
    assert!(matches!(status_error(503, "busy"), LlmError::Unavailable(_)));
    assert_eq!(status_error(400, "bad model"), LlmError::Failed("HTTP 400: bad model".into()));
}
