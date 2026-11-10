//! Unit tests for `wire.rs`.

use super::*;

#[test]
fn subject_replaces_slashes_and_adds_prefix() {
    let path = "/greet.v1.GreetService/Greet";
    assert_eq!(subject_for("rpc", path), "rpc.greet.v1.GreetService.Greet");
    assert_eq!(subject_for("", path), "greet.v1.GreetService.Greet");
}

#[test]
fn endpoint_name_is_short_unless_taken() {
    let mut taken = HashSet::new();
    let first = endpoint_name("/a.v1.GreetService/Greet", &mut taken);
    let second = endpoint_name("/b.v1.GreetService/Greet", &mut taken);
    assert_eq!(first, "GreetService-Greet");
    assert_eq!(second, "b_v1_GreetService_Greet");
}

#[test]
fn status_codes_map_to_connect_codes() {
    assert_eq!(code_for_status(400), "invalid_argument");
    assert_eq!(code_for_status(503), "unavailable");
    assert_eq!(code_for_status(418), "unknown");
}

#[test]
fn parses_connect_error_body() {
    let body = br#"{"code":"not_found","message":"no such user"}"#;
    assert_eq!(parse_connect_error(body), ("not_found".into(), "no such user".into()));
}

#[test]
fn empty_message_falls_back_to_code() {
    let body = br#"{"code":"internal"}"#;
    assert_eq!(parse_connect_error(body), ("internal".into(), "internal".into()));
}

#[test]
fn non_json_error_body_is_kept_as_message() {
    assert_eq!(parse_connect_error(b"boom"), ("unknown".into(), "boom".into()));
}

#[test]
fn service_name_is_module_name_without_instance() {
    assert_eq!(service_name("screen_sensor", None), "screen_sensor");
}

#[test]
fn service_name_appends_instance() {
    assert_eq!(service_name("screen_sensor", Some("screen-1")), "screen_sensor-screen-1");
}

#[test]
fn service_name_sanitizes_instance() {
    assert_eq!(service_name("camera_sensor", Some("cam 1.front")), "camera_sensor-cam_1_front");
}

#[test]
fn connect_plus_content_types_are_streaming() {
    assert!(is_streaming(Some("application/connect+json")));
    assert!(is_streaming(Some("application/connect+proto")));
}

#[test]
fn unary_request_to_a_streaming_method_gets_a_hint() {
    let hint = streaming_hint("unimplemented", "method not found: a.v1.S/Check", Some("application/json")).unwrap();
    assert!(hint.starts_with("a.v1.S/Check is a streaming method"), "{hint}");
    assert!(streaming_hint("unimplemented", "method not found: x", None).is_some(), "no content type = unary");
}

#[test]
fn other_errors_keep_their_message() {
    assert_eq!(streaming_hint("unimplemented", "method not found: x", Some("application/connect+json")), None);
    assert_eq!(streaming_hint("invalid_argument", "method not found: x", Some("application/json")), None);
}

#[test]
fn unary_or_missing_content_types_are_not_streaming() {
    assert!(!is_streaming(Some("application/json")));
    assert!(!is_streaming(Some("application/proto")));
    assert!(!is_streaming(None));
}
