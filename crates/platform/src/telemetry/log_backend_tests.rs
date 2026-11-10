//! Unit tests for `log_backend.rs`.

use super::*;
use kernel::kv;

#[test]
fn fields_become_a_json_object_keeping_types() {
    let json = fields_to_json(&[kv("name", "Jane"), kv("attempt", 2), kv("cached", false)]);
    assert_eq!(json, r#"{"name":"Jane","attempt":2,"cached":false}"#);
}

#[test]
fn no_fields_is_an_empty_object() {
    assert_eq!(fields_to_json(&[]), "{}");
}
