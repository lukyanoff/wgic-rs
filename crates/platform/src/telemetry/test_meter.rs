//! Test helper: a real OTel meter whose measurements can be read back.

use std::collections::HashMap;

use opentelemetry::metrics::{Meter, MeterProvider};
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, SdkMeterProvider};

pub struct TestMeter {
    provider: SdkMeterProvider,
    exporter: InMemoryMetricExporter,
}

impl TestMeter {
    pub fn new() -> Self {
        let exporter = InMemoryMetricExporter::default();
        let provider = SdkMeterProvider::builder()
            .with_periodic_exporter(exporter.clone())
            .build();
        Self { provider, exporter }
    }

    pub fn meter(&self) -> Meter {
        self.provider.meter("test")
    }

    /// Metric name → counter total, histogram count, or gauge value.
    pub fn totals(&self) -> HashMap<String, f64> {
        self.provider.force_flush().expect("flush metrics");
        let exports = self.exporter.get_finished_metrics().expect("read metrics");
        let mut totals = HashMap::new();
        let Some(latest) = exports.last() else {
            return totals;
        };
        for metric in latest.scope_metrics().flat_map(|scope| scope.metrics()) {
            totals.insert(metric.name().to_string(), total(metric.data()));
        }
        totals
    }
}

fn total(data: &AggregatedMetrics) -> f64 {
    match data {
        AggregatedMetrics::U64(MetricData::Sum(sum)) => {
            sum.data_points().map(|p| p.value() as f64).sum()
        }
        AggregatedMetrics::F64(MetricData::Histogram(histogram)) => {
            histogram.data_points().map(|p| p.count() as f64).sum()
        }
        AggregatedMetrics::F64(MetricData::Gauge(gauge)) => {
            gauge.data_points().map(|p| p.value()).last().unwrap_or(0.0)
        }
        _ => f64::NAN,
    }
}
