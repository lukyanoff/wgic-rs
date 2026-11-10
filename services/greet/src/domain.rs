//! Greeting rules. No IO, no async, no proto types.
//!
//! `Greeter` is the interface (≈ C# `IGreeter`), `HelloGreeter` the implementation (≈ `class Greeter : IGreeter`).
//! The handler depends on the interface, so tests can swap in a fake.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum GreetError {
    EmptyName,
    NameTooLong { max_chars: usize },
}

impl fmt::Display for GreetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GreetError::EmptyName => write!(f, "name must not be empty"),
            GreetError::NameTooLong { max_chars } => write!(f, "name must be at most {max_chars} characters"),
        }
    }
}

/// Builds a greeting for a person.
pub trait Greeter: Send + Sync {
    /// `"Jane"` → `"Hello, Jane!"`, or why the name is not acceptable.
    fn greet(&self, name: &str) -> Result<String, GreetError>;
}

/// The standard greeter: trims the name, rejects empty or too long names.
pub struct HelloGreeter {
    max_name_chars: usize,
}

impl HelloGreeter {
    pub const DEFAULT_MAX_NAME_CHARS: usize = 100;

    pub fn new() -> Self {
        Self::with_max_name_chars(Self::DEFAULT_MAX_NAME_CHARS)
    }

    pub fn with_max_name_chars(max_name_chars: usize) -> Self {
        Self { max_name_chars }
    }
}

impl Default for HelloGreeter {
    fn default() -> Self {
        Self::new()
    }
}

impl Greeter for HelloGreeter {
    fn greet(&self, name: &str) -> Result<String, GreetError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(GreetError::EmptyName);
        }
        if name.chars().count() > self.max_name_chars {
            return Err(GreetError::NameTooLong { max_chars: self.max_name_chars });
        }
        Ok(format!("Hello, {name}!"))
    }
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
