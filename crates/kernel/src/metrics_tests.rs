//! Unit tests for `metrics.rs`.

use super::*;
use crate::kv;
use crate::testing::MemoryMetrics;

fn metrics(memory: &MemoryMetrics) -> Metrics {
    Metrics::new("greet", Arc::new(memory.clone()))
}

#[test]
fn names_are_prefixed_with_the_meter() {
    let memory = MemoryMetrics::new();
    metrics(&memory).count("greetings", 2, []);
    assert_eq!(memory.sum("greet.greetings"), 2.0);
}

#[test]
fn each_method_records_its_kind() {
    let memory = MemoryMetrics::new();
    let m = metrics(&memory);
    m.count("calls", 1, []);
    m.record("latency_seconds", 0.25, []);
    m.set("queue_depth", 7.0, []);

    let kinds: Vec<MetricKind> = memory.points().iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            MetricKind::Counter,
            MetricKind::Histogram,
            MetricKind::Gauge
        ]
    );
    assert_eq!(memory.last("greet.queue_depth"), Some(7.0));
}

#[test]
fn tags_are_passed_through() {
    let memory = MemoryMetrics::new();
    metrics(&memory).count("greetings", 1, [kv("transport", "nats")]);
    assert_eq!(memory.points()[0].tags, [kv("transport", "nats")]);
}
