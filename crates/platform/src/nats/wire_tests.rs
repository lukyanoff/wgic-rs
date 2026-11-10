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
