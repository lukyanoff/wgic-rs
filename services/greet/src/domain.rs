//! Greeting rules. Pure functions: no IO, no async.

use std::fmt;

pub const MAX_NAME_CHARS: usize = 100;

#[derive(Debug, PartialEq, Eq)]
pub enum GreetError {
    EmptyName,
    NameTooLong,
}

impl fmt::Display for GreetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GreetError::EmptyName => write!(f, "name must not be empty"),
            GreetError::NameTooLong => {
                write!(f, "name must be at most {MAX_NAME_CHARS} characters")
            }
        }
    }
}

/// `"Jane"` → `"Hello, Jane!"`. Surrounding whitespace is ignored.
pub fn greeting(name: &str) -> Result<String, GreetError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(GreetError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(GreetError::NameTooLong);
    }
    Ok(format!("Hello, {name}!"))
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
