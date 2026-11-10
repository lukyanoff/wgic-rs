use opentelemetry::propagation::TextMapCompositePropagator;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::propagation::{BaggagePropagator, TraceContextPropagator};
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::filter::{EnvFilter, filter_fn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt as tfmt};

use super::JsonLogFormat;

/// Flushes and shuts down the OTEL providers when dropped. Keep it alive for the whole `main`.
#[must_use = "dropping the guard shuts telemetry down"]
pub struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
    logger_provider: SdkLoggerProvider,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        let _ = self.tracer_provider.force_flush();
        let _ = self.logger_provider.force_flush();
        let _ = self.tracer_provider.shutdown();
        let _ = self.logger_provider.shutdown();
    }
}

/// Install tracing + OpenTelemetry for this process.
///
/// - `OTEL_SERVICE_NAME` overrides `default_service`.
/// - `OTEL_EXPORTER_OTLP_ENDPOINT` (e.g. `http://127.0.0.1:4318`) turns on OTLP/HTTP export of
///   traces and logs. Without it, spans still get real IDs, so logs stay correlated.
/// - `RUST_LOG` sets the log filter (default `info`).
pub fn init(default_service: &str) -> TelemetryGuard {
    global::set_text_map_propagator(TextMapCompositePropagator::new(vec![
        Box::new(TraceContextPropagator::new()),
        Box::new(BaggagePropagator::new()),
    ]));
    let resource = resource(default_service);
    let tracer_provider = tracer_provider(resource.clone());
    let logger_provider = logger_provider(resource);
    global::set_tracer_provider(tracer_provider.clone());
    install_subscriber(&tracer_provider, &logger_provider);
    TelemetryGuard {
        tracer_provider,
        logger_provider,
    }
}

fn env_set(key: &str) -> bool {
    std::env::var(key).is_ok_and(|v| !v.trim().is_empty())
}

fn resource(default_service: &str) -> Resource {
    let service = std::env::var("OTEL_SERVICE_NAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_service.to_string());
    Resource::builder_empty()
        .with_service_name(service)
        .with_attributes([
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("service.instance.id", uuid::Uuid::new_v4().to_string()),
        ])
        .build()
}

fn tracer_provider(resource: Resource) -> SdkTracerProvider {
    let builder = SdkTracerProvider::builder().with_resource(resource);
    if !env_set("OTEL_EXPORTER_OTLP_ENDPOINT") && !env_set("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT") {
        return builder.build();
    }
    match opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()
    {
        Ok(exporter) => builder.with_batch_exporter(exporter).build(),
        Err(e) => {
            eprintln!("otel: cannot build span exporter: {e}");
            builder.build()
        }
    }
}

fn logger_provider(resource: Resource) -> SdkLoggerProvider {
    let builder = SdkLoggerProvider::builder().with_resource(resource);
    if !env_set("OTEL_EXPORTER_OTLP_ENDPOINT") && !env_set("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT") {
        return builder.build();
    }
    match opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .build()
    {
        Ok(exporter) => builder.with_batch_exporter(exporter).build(),
        Err(e) => {
            eprintln!("otel: cannot build log exporter: {e}");
            builder.build()
        }
    }
}

fn install_subscriber(tracer_provider: &SdkTracerProvider, logger_provider: &SdkLoggerProvider) {
    // Never feed the exporters' own diagnostics back into the exporters.
    let not_otel_internals = filter_fn(|meta| {
        let target = meta.target();
        !["opentelemetry", "hyper", "reqwest", "h2"]
            .iter()
            .any(|prefix| target.starts_with(prefix))
    });
    let spans = tracing_opentelemetry::layer()
        .with_tracer(tracer_provider.tracer(env!("CARGO_PKG_NAME")))
        .with_error_events_to_status(true);
    let otel_logs =
        OpenTelemetryTracingBridge::new(logger_provider).with_filter(not_otel_internals);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // `try_init`: a second call (e.g. two tests in one process) keeps the first subscriber.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(spans)
        .with(otel_logs)
        .with(tfmt::layer().event_format(JsonLogFormat))
        .try_init();
}
