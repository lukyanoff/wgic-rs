//! OpenTelemetry for every process: traces → Jaeger, logs → collector, metrics → Prometheus/Grafana.
//!
//! - [`init_telemetry`] installs the `tracing` subscriber (console logs + spans/logs as OTLP). Export is on when
//!   `OTEL_EXPORTER_OTLP_ENDPOINT` is set (e.g. `http://127.0.0.1:4318`, see `scripts/run-all.sh`); without it,
//!   spans still get real ids, so trace ids still flow through NATS headers.
//! - [`inject`] / [`extract`] carry W3C `traceparent` in NATS headers, so one trace spans every hop.

use std::time::Duration;

use opentelemetry::propagation::{Extractor, Injector, TextMapCompositePropagator};
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry::{Context, KeyValue, global};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::propagation::{BaggagePropagator, TraceContextPropagator};
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{EnvFilter, filter_fn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

// ============================================================================
// Setup
// ============================================================================

/// Flushes and shuts down the exporters when dropped (≈ C# `IDisposable`). Keep it alive for the whole `main`.
#[must_use = "dropping the guard shuts telemetry down"]
pub struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
    logger_provider: SdkLoggerProvider,
    meter_provider: SdkMeterProvider,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        let _ = self.tracer_provider.shutdown();
        let _ = self.logger_provider.shutdown();
        let _ = self.meter_provider.shutdown();
    }
}

/// Install tracing + OpenTelemetry for this process. `service.name` = `module`, `wgic.instance` = `instance`.
pub fn init_telemetry(module: &str, instance: &str) -> TelemetryGuard {
    global::set_text_map_propagator(TextMapCompositePropagator::new(vec![
        Box::new(TraceContextPropagator::new()),
        Box::new(BaggagePropagator::new()),
    ]));
    let resource = Resource::builder_empty()
        .with_service_name(module.to_string())
        .with_attributes([
            KeyValue::new("wgic.instance", instance.to_string()),
            KeyValue::new("service.instance.id", uuid::Uuid::new_v4().to_string()),
        ])
        .build();
    let export = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok_and(|v| !v.trim().is_empty());
    let guard = TelemetryGuard {
        tracer_provider: tracer_provider(resource.clone(), export),
        logger_provider: logger_provider(resource.clone(), export),
        meter_provider: meter_provider(resource, export),
    };
    global::set_tracer_provider(guard.tracer_provider.clone());
    global::set_meter_provider(guard.meter_provider.clone());
    install_subscriber(&guard);
    tracing::info!(module, instance, otlp_export = export, "telemetry initialized");
    guard
}

fn tracer_provider(resource: Resource, export: bool) -> SdkTracerProvider {
    let builder = SdkTracerProvider::builder().with_resource(resource);
    match export.then(|| opentelemetry_otlp::SpanExporter::builder().with_http().build()) {
        Some(Ok(exporter)) => builder.with_batch_exporter(exporter).build(),
        Some(Err(e)) => {
            eprintln!("otel: cannot build span exporter: {e}");
            builder.build()
        }
        None => builder.build(),
    }
}

fn logger_provider(resource: Resource, export: bool) -> SdkLoggerProvider {
    let builder = SdkLoggerProvider::builder().with_resource(resource);
    match export.then(|| opentelemetry_otlp::LogExporter::builder().with_http().build()) {
        Some(Ok(exporter)) => builder.with_batch_exporter(exporter).build(),
        Some(Err(e)) => {
            eprintln!("otel: cannot build log exporter: {e}");
            builder.build()
        }
        None => builder.build(),
    }
}

/// Metrics are pushed every 10 s (the SDK's default 60 s is slow for local dev).
fn meter_provider(resource: Resource, export: bool) -> SdkMeterProvider {
    let builder = SdkMeterProvider::builder().with_resource(resource);
    match export.then(|| opentelemetry_otlp::MetricExporter::builder().with_http().build()) {
        Some(Ok(exporter)) => {
            let reader = PeriodicReader::builder(exporter).with_interval(Duration::from_secs(10)).build();
            builder.with_reader(reader).build()
        }
        Some(Err(e)) => {
            eprintln!("otel: cannot build metric exporter: {e}");
            builder.build()
        }
        None => builder.build(),
    }
}

/// `RUST_LOG` filter (default `info`) → console + OTEL spans + OTLP logs.
fn install_subscriber(guard: &TelemetryGuard) {
    // Never feed the exporters' own diagnostics back into the exporters.
    let not_otel_internals = filter_fn(|meta| {
        !["opentelemetry", "hyper", "reqwest", "h2"].iter().any(|prefix| meta.target().starts_with(prefix))
    });
    let spans = tracing_opentelemetry::layer()
        .with_tracer(guard.tracer_provider.tracer("wgic"))
        .with_error_events_to_status(true);
    let otel_logs = OpenTelemetryTracingBridge::new(&guard.logger_provider).with_filter(not_otel_internals);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(spans)
        .with(otel_logs)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .try_init();
}

// ============================================================================
// W3C trace context in NATS headers
// ============================================================================

/// Read `traceparent` etc. from NATS headers (case-insensitive).
struct HeaderReader<'a>(&'a async_nats::HeaderMap);

impl Extractor for HeaderReader<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        let mut entries = self.0.iter();
        let (_, values) = entries.find(|(k, _)| AsRef::<str>::as_ref(*k).eq_ignore_ascii_case(key))?;
        values.first().map(|v| v.as_str())
    }
    fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(k, _)| AsRef::<str>::as_ref(k)).collect()
    }
}

/// Write `traceparent` etc. into NATS headers.
struct HeaderWriter<'a>(&'a mut async_nats::HeaderMap);

impl Injector for HeaderWriter<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key, value.as_str());
    }
}

/// Write the current span's trace context into `headers`.
pub fn inject(span: &tracing::Span, headers: &mut async_nats::HeaderMap) {
    let cx = span.context();
    global::get_text_map_propagator(|p| p.inject_context(&cx, &mut HeaderWriter(headers)));
}

/// Make the caller's span (from `traceparent`) the parent of `span`, when the request carries one.
pub fn continue_trace(span: &tracing::Span, headers: Option<&async_nats::HeaderMap>) {
    let Some(headers) = headers else { return };
    let parent: Context = global::get_text_map_propagator(|p| p.extract(&HeaderReader(headers)));
    if parent.span().span_context().is_valid() {
        let _ = span.set_parent(parent);
    }
}
