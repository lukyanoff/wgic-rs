//! Unit tests for the rules in `handler.rs`.

use connectrpc::ErrorCode;

use super::*;

#[test]
fn greets_by_trimmed_name() {
    assert_eq!(greeting("  Jane ").unwrap(), "Hello, Jane!");
}

#[test]
fn empty_name_is_invalid_argument() {
    let error = greeting("   ").unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert_eq!(error.message.as_deref(), Some("name must not be empty"));
}

#[test]
fn name_length_counts_characters_not_bytes() {
    assert!(greeting(&"я".repeat(MAX_NAME_CHARS)).is_ok());
    assert!(greeting(&"a".repeat(MAX_NAME_CHARS + 1)).is_err());
}
