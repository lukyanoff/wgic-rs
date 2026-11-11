//! `ILogger`: the logger a service gets through its constructor (≈ C# `ILogger<T>`).
//!
//! ```ignore
//! self.log.info("frame captured", json!({ "bytes": size, "mime_type": mime }));
//! ```
//!
//! Every entry also carries, without the service doing anything:
//! - `service.name`, `wgic.instance` (the process, from the OTel resource)
//! - `trace_id`, `span_id`, `rpc.service`, `rpc.method` (the call being served), `wgic.target` (a downstream call)
//!
//! Field names: snake_case for domain fields, OTel names where one exists (`error.type`, `error.message`,
//! `duration_ms`). Never repeat module, instance or method: the context adds them.

use std::sync::Mutex;
use tracing::{Level as TL, event};

pub use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

// ============================================================================
// The interface (≈ C# `interface ILogger`)
// ============================================================================

pub trait ILogger: Send + Sync + 'static {
    /// The one method an implementation writes; `fields` is a JSON object (`json!({})` for none).
    fn write(&self, level: Level, message: &str, fields: Value);

    fn debug(&self, message: &str, fields: Value) {
        self.write(Level::Debug, message, fields)
    }
    fn info(&self, message: &str, fields: Value) {
        self.write(Level::Info, message, fields)
    }
    fn warn(&self, message: &str, fields: Value) {
        self.write(Level::Warn, message, fields)
    }
    fn error(&self, message: &str, fields: Value) {
        self.write(Level::Error, message, fields)
    }
}

// ============================================================================
// Production: tracing → console + OTLP (collector → Loki)
// ============================================================================

/// `category` ≈ the `T` in `ILogger<T>`: usually the module name (`sensor`).
pub struct OtelLogger {
    category: String,
}

impl OtelLogger {
    pub fn new(category: &str) -> Self {
        Self { category: category.to_string() }
    }
}

impl ILogger for OtelLogger {
    fn write(&self, level: Level, message: &str, fields: Value) {
        let category = self.category.as_str();
        // `tracing` wants field names at compile time; ours arrive at run time. So they travel as one JSON string,
        // `log_fields`, and the collector turns them back into attributes (otel/collector.yaml). Not `log.fields`:
        // the console formatter hides `log.*` fields.
        let fields = fields.to_string();
        match level {
            Level::Debug => {
                event!(target: "service", TL::DEBUG, log_category = category, log_fields = %fields, "{message}")
            }
            Level::Info => {
                event!(target: "service", TL::INFO, log_category = category, log_fields = %fields, "{message}")
            }
            Level::Warn => {
                event!(target: "service", TL::WARN, log_category = category, log_fields = %fields, "{message}")
            }
            Level::Error => {
                event!(target: "service", TL::ERROR, log_category = category, log_fields = %fields, "{message}")
            }
        }
    }
}

// ============================================================================
// Tests: remembers every entry (≈ a fake `ILogger` in xUnit)
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub level: Level,
    pub message: String,
    pub fields: Value,
}

#[derive(Default)]
pub struct MemoryLogger {
    entries: Mutex<Vec<LogEntry>>,
}

impl MemoryLogger {
    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.lock().expect("logger lock poisoned").clone()
    }
}

impl ILogger for MemoryLogger {
    fn write(&self, level: Level, message: &str, fields: Value) {
        let entry = LogEntry { level, message: message.to_string(), fields };
        self.entries.lock().expect("logger lock poisoned").push(entry);
    }
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod log_tests;
