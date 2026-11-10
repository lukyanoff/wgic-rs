//! Unit tests for `domain.rs`.

use super::*;

fn greeter() -> HelloGreeter {
    HelloGreeter::new()
}

#[test]
fn greets_by_name() {
    assert_eq!(greeter().greet("Jane"), Ok("Hello, Jane!".to_string()));
}

#[test]
fn trims_whitespace() {
    assert_eq!(greeter().greet("  Jane "), Ok("Hello, Jane!".to_string()));
}

#[test]
fn rejects_empty_name() {
    assert_eq!(greeter().greet("   "), Err(GreetError::EmptyName));
}

#[test]
fn rejects_long_name() {
    let long = "a".repeat(HelloGreeter::DEFAULT_MAX_NAME_CHARS + 1);
    assert_eq!(greeter().greet(&long), Err(GreetError::NameTooLong { max_chars: 100 }));
}

#[test]
fn counts_characters_not_bytes() {
    let cyrillic = "я".repeat(HelloGreeter::DEFAULT_MAX_NAME_CHARS);
    assert!(greeter().greet(&cyrillic).is_ok());
}

#[test]
fn max_length_is_configurable() {
    let strict = HelloGreeter::with_max_name_chars(3);
    assert!(strict.greet("Ann").is_ok());
    assert_eq!(strict.greet("Anna"), Err(GreetError::NameTooLong { max_chars: 3 }));
}

#[test]
fn error_message_names_the_limit() {
    let error = GreetError::NameTooLong { max_chars: 3 };
    assert_eq!(error.to_string(), "name must be at most 3 characters");
}
