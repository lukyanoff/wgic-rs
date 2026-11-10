//! Structured key/value pairs for logs and metric tags: `kv("name", "Jane")`.

use std::fmt;

/// A value in a log field or metric tag.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Str(String),
    I64(i64),
    F64(f64),
    Bool(bool),
}

/// One key/value pair. Build it with [`kv`].
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub key: &'static str,
    pub value: FieldValue,
}

/// `kv("name", "Jane")`, `kv("attempt", 3)`, `kv("cached", true)`.
pub fn kv(key: &'static str, value: impl Into<FieldValue>) -> Field {
    Field { key, value: value.into() }
}

impl fmt::Display for FieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldValue::Str(v) => write!(f, "{v}"),
            FieldValue::I64(v) => write!(f, "{v}"),
            FieldValue::F64(v) => write!(f, "{v}"),
            FieldValue::Bool(v) => write!(f, "{v}"),
        }
    }
}

impl From<&str> for FieldValue {
    fn from(v: &str) -> Self {
        FieldValue::Str(v.to_string())
    }
}

impl From<String> for FieldValue {
    fn from(v: String) -> Self {
        FieldValue::Str(v)
    }
}

impl From<&String> for FieldValue {
    fn from(v: &String) -> Self {
        FieldValue::Str(v.clone())
    }
}

impl From<i32> for FieldValue {
    fn from(v: i32) -> Self {
        FieldValue::I64(v.into())
    }
}

impl From<i64> for FieldValue {
    fn from(v: i64) -> Self {
        FieldValue::I64(v)
    }
}

impl From<u32> for FieldValue {
    fn from(v: u32) -> Self {
        FieldValue::I64(v.into())
    }
}

/// Values above `i64::MAX` are kept as `F64` (approximate) rather than wrapping around.
impl From<u64> for FieldValue {
    fn from(v: u64) -> Self {
        match i64::try_from(v) {
            Ok(v) => FieldValue::I64(v),
            Err(_) => FieldValue::F64(v as f64),
        }
    }
}

impl From<usize> for FieldValue {
    fn from(v: usize) -> Self {
        FieldValue::from(v as u64)
    }
}

impl From<f64> for FieldValue {
    fn from(v: f64) -> Self {
        FieldValue::F64(v)
    }
}

impl From<bool> for FieldValue {
    fn from(v: bool) -> Self {
        FieldValue::Bool(v)
    }
}

#[cfg(test)]
#[path = "field_tests.rs"]
mod tests;
