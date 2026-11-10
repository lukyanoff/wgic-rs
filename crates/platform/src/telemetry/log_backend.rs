//! `TracingLogs`: sends service logs (`kernel::Logger`) into `tracing`, so they get the same
//! JSON stdout format, `trace_id` correlation and OTLP export as everything else.

use kernel::{Field, FieldValue, Level, LogBackend};
use serde_json::{Map, Value};

/// Field that carries a service log's key/value pairs as one JSON object.
/// `tracing` needs field names at compile time, service fields are only known at runtime,
/// so they travel together and are flattened again by `JsonLogFormat` and the collector.
pub const SERVICE_FIELDS: &str = "log.fields";

/// The production [`LogBackend`].
pub struct TracingLogs;

impl LogBackend for TracingLogs {
    fn log(&self, level: Level, category: &str, message: &str, fields: &[Field]) {
        let fields = fields_to_json(fields);
        match level {
            Level::Debug => {
                tracing::debug!(target: "service", category, log.fields = %fields, "{message}")
            }
            Level::Info => {
                tracing::info!(target: "service", category, log.fields = %fields, "{message}")
            }
            Level::Warn => {
                tracing::warn!(target: "service", category, log.fields = %fields, "{message}")
            }
            Level::Error => {
                tracing::error!(target: "service", category, log.fields = %fields, "{message}")
            }
        }
    }
}

/// `[kv("name", "Jane"), kv("attempt", 2)]` → `{"name":"Jane","attempt":2}`
pub fn fields_to_json(fields: &[Field]) -> String {
    let mut object = Map::new();
    for field in fields {
        object.insert(field.key.to_string(), to_json(&field.value));
    }
    Value::Object(object).to_string()
}

fn to_json(value: &FieldValue) -> Value {
    match value {
        FieldValue::Str(v) => Value::from(v.as_str()),
        FieldValue::I64(v) => Value::from(*v),
        FieldValue::F64(v) => Value::from(*v),
        FieldValue::Bool(v) => Value::from(*v),
    }
}

#[cfg(test)]
#[path = "log_backend_tests.rs"]
mod tests;
