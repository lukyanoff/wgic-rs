//! Unit tests for `metrics_backend.rs`.

use super::*;
use crate::telemetry::test_meter::TestMeter;
use kernel::kv;

#[test]
fn counters_histograms_and_gauges_reach_otel() {
    let test = TestMeter::new();
    let backend = OtelMetrics::with_meter(test.meter());

    backend.record(MetricKind::Counter, "greet.greetings", 2.0, &[kv("transport", "nats")]);
    backend.record(MetricKind::Counter, "greet.greetings", 1.0, &[kv("transport", "http")]);
    backend.record(MetricKind::Histogram, "greet.work_seconds", 0.2, &[]);
    backend.record(MetricKind::Gauge, "greet.queue_depth", 5.0, &[]);

    let totals = test.totals();
    assert_eq!(totals["greet.greetings"], 3.0);
    assert_eq!(totals["greet.work_seconds"], 1.0);
    assert_eq!(totals["greet.queue_depth"], 5.0);
}

#[test]
fn reusing_a_name_with_another_kind_is_ignored() {
    let test = TestMeter::new();
    let backend = OtelMetrics::with_meter(test.meter());

    backend.record(MetricKind::Counter, "greet.calls", 1.0, &[]);
    backend.record(MetricKind::Gauge, "greet.calls", 99.0, &[]);

    assert_eq!(test.totals()["greet.calls"], 1.0);
}

#[test]
fn fields_keep_their_types_as_attributes() {
    let attributes = to_key_values(&[kv("transport", "nats"), kv("retries", 2)]);
    assert_eq!(attributes[0], KeyValue::new("transport", "nats"));
    assert_eq!(attributes[1], KeyValue::new("retries", 2_i64));
}
