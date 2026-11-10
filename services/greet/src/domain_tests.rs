//! Unit tests for `domain.rs`.

use super::*;

#[test]
fn greets_by_name() {
    assert_eq!(greeting("Jane"), Ok("Hello, Jane!".to_string()));
}

#[test]
fn trims_whitespace() {
    assert_eq!(greeting("  Jane "), Ok("Hello, Jane!".to_string()));
}

#[test]
fn rejects_empty_name() {
    assert_eq!(greeting("   "), Err(GreetError::EmptyName));
}

#[test]
fn rejects_long_name() {
    let long = "a".repeat(MAX_NAME_CHARS + 1);
    assert_eq!(greeting(&long), Err(GreetError::NameTooLong));
}

#[test]
fn counts_characters_not_bytes() {
    let cyrillic = "я".repeat(MAX_NAME_CHARS);
    assert!(greeting(&cyrillic).is_ok());
}
