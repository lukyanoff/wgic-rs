//! Backends for tests and defaults: record in memory, or do nothing.
//!
//! ```ignore
//! let logs = MemoryLogs::new();
//! let metrics = MemoryMetrics::new();
//! let deps = Deps::in_process(&[]).with_logs(logs.clone()).with_metrics(metrics.clone());
//! // ... call the service ...
//! assert!(logs.find("greeting sent").is_some());
//! assert_eq!(metrics.sum("greet.greetings"), 1.0);
//! ```

use std::sync::{Arc, Mutex};

use crate::{Field, FieldValue, Level, LogBackend, MetricKind, MetricsBackend};

/// One captured log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: Level,
    pub category: String,
    pub message: String,
    pub fields: Vec<Field>,
}

impl LogEntry {
    /// The value of field `key`, if present.
    pub fn field(&self, key: &str) -> Option<&FieldValue> {
        self.fields.iter().find(|f| f.key == key).map(|f| &f.value)
    }
}

/// Keeps every log entry in memory. Clones share the same list.
#[derive(Clone, Default)]
pub struct MemoryLogs {
    entries: Arc<Mutex<Vec<LogEntry>>>,
}

impl MemoryLogs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.lock().expect("logs lock").clone()
    }

    pub fn messages(&self) -> Vec<String> {
        self.entries().into_iter().map(|e| e.message).collect()
    }

    /// The first entry with exactly this message.
    pub fn find(&self, message: &str) -> Option<LogEntry> {
        self.entries().into_iter().find(|e| e.message == message)
    }
}

impl LogBackend for MemoryLogs {
    fn log(&self, level: Level, category: &str, message: &str, fields: &[Field]) {
        let entry =
            LogEntry { level, category: category.to_string(), message: message.to_string(), fields: fields.to_vec() };
        self.entries.lock().expect("logs lock").push(entry);
    }
}

/// One captured measurement.
#[derive(Debug, Clone)]
pub struct MetricPoint {
    pub kind: MetricKind,
    pub name: String,
    pub value: f64,
    pub tags: Vec<Field>,
}

/// Keeps every measurement in memory. Clones share the same list.
#[derive(Clone, Default)]
pub struct MemoryMetrics {
    points: Arc<Mutex<Vec<MetricPoint>>>,
}

impl MemoryMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn points(&self) -> Vec<MetricPoint> {
        self.points.lock().expect("metrics lock").clone()
    }

    /// Sum of all values recorded under `name` (full name, e.g. `greet.greetings`).
    pub fn sum(&self, name: &str) -> f64 {
        self.points().iter().filter(|p| p.name == name).map(|p| p.value).sum()
    }

    /// The last value recorded under `name`.
    pub fn last(&self, name: &str) -> Option<f64> {
        self.points().iter().rev().find(|p| p.name == name).map(|p| p.value)
    }
}

impl MetricsBackend for MemoryMetrics {
    fn record(&self, kind: MetricKind, name: &str, value: f64, tags: &[Field]) {
        let point = MetricPoint { kind, name: name.to_string(), value, tags: tags.to_vec() };
        self.points.lock().expect("metrics lock").push(point);
    }
}

/// Drops every log entry. The default in `Deps`.
pub struct NoopLogs;

impl LogBackend for NoopLogs {
    fn log(&self, _: Level, _: &str, _: &str, _: &[Field]) {}
}

/// Drops every measurement. The default in `Deps`.
pub struct NoopMetrics;

impl MetricsBackend for NoopMetrics {
    fn record(&self, _: MetricKind, _: &str, _: f64, _: &[Field]) {}
}
