//! `OtelMetrics`: sends service metrics (`kernel::Metrics`) to OpenTelemetry
//! (→ OTLP → collector → Prometheus). Instruments are created on first use of a name.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use kernel::{Field, FieldValue, MetricKind, MetricsBackend};
use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter};
use opentelemetry::{KeyValue, global};

/// Histogram buckets for durations in seconds (1 ms … 10 s).
pub const SECONDS_BUCKETS: [f64; 13] = [0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

#[derive(Clone)]
enum Instrument {
    Counter(Counter<u64>),
    Histogram(Histogram<f64>),
    Gauge(Gauge<f64>),
}

#[derive(Default)]
struct Registry {
    instruments: HashMap<String, Instrument>,
    /// Names already reported as used with two different kinds.
    conflicts: HashSet<String>,
}

/// The production [`MetricsBackend`].
pub struct OtelMetrics {
    meter: Meter,
    registry: Mutex<Registry>,
}

impl OtelMetrics {
    /// Uses the global meter provider installed by `telemetry::init`.
    pub fn new() -> Self {
        Self::with_meter(global::meter("services"))
    }

    pub fn with_meter(meter: Meter) -> Self {
        Self { meter, registry: Mutex::new(Registry::default()) }
    }

    /// The instrument for `name`, created on first use. `None` if `name` was first
    /// used with a different kind (that mistake is logged once).
    fn instrument(&self, kind: MetricKind, name: &str) -> Option<Instrument> {
        let mut registry = self.registry.lock().expect("metrics registry lock");
        let instrument =
            registry.instruments.entry(name.to_string()).or_insert_with(|| create(&self.meter, kind, name)).clone();
        if kind_of(&instrument) == kind {
            return Some(instrument);
        }
        if registry.conflicts.insert(name.to_string()) {
            tracing::warn!(metric = name, ?kind, "metric name already used with another kind; ignored");
        }
        None
    }
}

impl Default for OtelMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsBackend for OtelMetrics {
    fn record(&self, kind: MetricKind, name: &str, value: f64, tags: &[Field]) {
        let Some(instrument) = self.instrument(kind, name) else {
            return;
        };
        let attributes = to_key_values(tags);
        match instrument {
            Instrument::Counter(c) => c.add(value as u64, &attributes),
            Instrument::Histogram(h) => h.record(value, &attributes),
            Instrument::Gauge(g) => g.record(value, &attributes),
        }
    }
}

fn create(meter: &Meter, kind: MetricKind, name: &str) -> Instrument {
    let name = name.to_string();
    match kind {
        MetricKind::Counter => Instrument::Counter(meter.u64_counter(name).build()),
        MetricKind::Gauge => Instrument::Gauge(meter.f64_gauge(name).build()),
        MetricKind::Histogram if name.ends_with("_seconds") => {
            let histogram = meter.f64_histogram(name).with_unit("s").with_boundaries(SECONDS_BUCKETS.to_vec());
            Instrument::Histogram(histogram.build())
        }
        MetricKind::Histogram => Instrument::Histogram(meter.f64_histogram(name).build()),
    }
}

fn kind_of(instrument: &Instrument) -> MetricKind {
    match instrument {
        Instrument::Counter(_) => MetricKind::Counter,
        Instrument::Histogram(_) => MetricKind::Histogram,
        Instrument::Gauge(_) => MetricKind::Gauge,
    }
}

pub fn to_key_values(fields: &[Field]) -> Vec<KeyValue> {
    let mut attributes = Vec::with_capacity(fields.len());
    for field in fields {
        let key = field.key;
        let attribute = match &field.value {
            FieldValue::Str(v) => KeyValue::new(key, v.clone()),
            FieldValue::I64(v) => KeyValue::new(key, *v),
            FieldValue::F64(v) => KeyValue::new(key, *v),
            FieldValue::Bool(v) => KeyValue::new(key, *v),
        };
        attributes.push(attribute);
    }
    attributes
}

#[cfg(test)]
#[path = "metrics_backend_tests.rs"]
mod tests;
