//! Unit tests for `field.rs`.

use super::*;

#[test]
fn strings_numbers_and_bools_convert() {
    assert_eq!(kv("name", "Jane").value, FieldValue::Str("Jane".into()));
    assert_eq!(kv("name", String::from("Jane")).value, FieldValue::Str("Jane".into()));
    assert_eq!(kv("attempt", 3).value, FieldValue::I64(3));
    assert_eq!(kv("ratio", 0.5).value, FieldValue::F64(0.5));
    assert_eq!(kv("cached", true).value, FieldValue::Bool(true));
}

#[test]
fn huge_u64_does_not_wrap_around() {
    assert_eq!(kv("n", u64::MAX).value, FieldValue::F64(u64::MAX as f64));
    assert_eq!(kv("n", 7_u64).value, FieldValue::I64(7));
}

#[test]
fn displays_plain_value() {
    assert_eq!(FieldValue::Str("x".into()).to_string(), "x");
    assert_eq!(FieldValue::I64(42).to_string(), "42");
}
