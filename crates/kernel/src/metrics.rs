//! `Metrics`: the injected metrics API services use (≈ C# `IMetrics`).

use std::sync::Arc;

use crate::Field;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricKind {
    /// Only goes up: requests, rejections, items processed.
    Counter,
    /// A distribution: durations, sizes.
    Histogram,
    /// The current value: queue depth, cache size.
    Gauge,
}

/// Where measurements go. Implemented by platform (OTel → Prometheus) and by
/// `testing::MemoryMetrics`. Services never implement or call this directly.
pub trait MetricsBackend: Send + Sync {
    fn record(&self, kind: MetricKind, name: &str, value: f64, tags: &[Field]);
}

/// Metrics for one service. Names are short; the service name is added in front:
/// `deps.metrics("greet").count("greetings", 1, [])` records `greet.greetings`.
///
/// Tags must have few distinct values (transport, reason, status) — never user names or ids.
#[derive(Clone)]
pub struct Metrics {
    meter: String,
    backend: Arc<dyn MetricsBackend>,
}

impl Metrics {
    pub fn new(meter: &str, backend: Arc<dyn MetricsBackend>) -> Self {
        Self { meter: meter.to_string(), backend }
    }

    /// Add `value` to a counter.
    pub fn count(&self, name: &str, value: u64, tags: impl IntoIterator<Item = Field>) {
        self.send(MetricKind::Counter, name, value as f64, tags);
    }

    /// Record one value in a histogram. End the name with `_seconds` for durations.
    pub fn record(&self, name: &str, value: f64, tags: impl IntoIterator<Item = Field>) {
        self.send(MetricKind::Histogram, name, value, tags);
    }

    /// Set a gauge to its current value.
    pub fn set(&self, name: &str, value: f64, tags: impl IntoIterator<Item = Field>) {
        self.send(MetricKind::Gauge, name, value, tags);
    }

    fn send(&self, kind: MetricKind, name: &str, value: f64, tags: impl IntoIterator<Item = Field>) {
        let full_name = format!("{}.{name}", self.meter);
        let tags: Vec<Field> = tags.into_iter().collect();
        self.backend.record(kind, &full_name, value, &tags);
    }
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
