//! `Logger`: the injected logger services use (≈ C# `ILogger<T>`).

use std::sync::Arc;

use crate::Field;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

/// Where log entries go. Implemented by platform (JSON stdout + OTel) and by
/// `testing::MemoryLogs`. Services never implement or call this directly.
pub trait LogBackend: Send + Sync {
    fn log(&self, level: Level, category: &str, message: &str, fields: &[Field]);
}

/// A logger for one category (usually the service name). Cheap to clone.
///
/// ```ignore
/// let log = deps.logger("greet");
/// log.info("greeting sent", [kv("name", "Jane")]);
/// let order_log = log.with([kv("order_id", 42)]);   // ≈ BeginScope: added to every entry
/// ```
#[derive(Clone)]
pub struct Logger {
    category: String,
    scope: Vec<Field>,
    backend: Arc<dyn LogBackend>,
}

impl Logger {
    pub fn new(category: &str, backend: Arc<dyn LogBackend>) -> Self {
        Self { category: category.to_string(), scope: Vec::new(), backend }
    }

    /// A logger that adds `fields` to every entry it writes.
    pub fn with(&self, fields: impl IntoIterator<Item = Field>) -> Logger {
        let mut child = self.clone();
        child.scope.extend(fields);
        child
    }

    pub fn debug(&self, message: &str, fields: impl IntoIterator<Item = Field>) {
        self.write(Level::Debug, message, fields);
    }

    pub fn info(&self, message: &str, fields: impl IntoIterator<Item = Field>) {
        self.write(Level::Info, message, fields);
    }

    pub fn warn(&self, message: &str, fields: impl IntoIterator<Item = Field>) {
        self.write(Level::Warn, message, fields);
    }

    pub fn error(&self, message: &str, fields: impl IntoIterator<Item = Field>) {
        self.write(Level::Error, message, fields);
    }

    fn write(&self, level: Level, message: &str, fields: impl IntoIterator<Item = Field>) {
        let mut all = self.scope.clone();
        all.extend(fields);
        self.backend.log(level, &self.category, message, &all);
    }
}

#[cfg(test)]
#[path = "logger_tests.rs"]
mod tests;
