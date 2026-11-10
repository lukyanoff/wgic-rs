//! Unit tests for `logs.rs`.

use super::*;
use serde_json::json;

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn service_fields_are_flattened() {
    let mut out = object(json!({"message": "hi", "log.fields": r#"{"name":"Jane","n":2}"#}));
    flatten_service_fields(&mut out);
    assert_eq!(Value::Object(out), json!({"message": "hi", "name": "Jane", "n": 2}));
}

#[test]
fn service_fields_never_overwrite_standard_keys() {
    let mut out = object(json!({"message": "hi", "log.fields": r#"{"message":"spoofed"}"#}));
    flatten_service_fields(&mut out);
    assert_eq!(out["message"], "hi");
}

#[test]
fn lines_without_service_fields_are_unchanged() {
    let mut out = object(json!({"message": "hi"}));
    flatten_service_fields(&mut out);
    assert_eq!(Value::Object(out), json!({"message": "hi"}));
}
