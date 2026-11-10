//! OpenTelemetry for every RPC, on every protocol and transport, and JSON logs
//! correlated with the active trace.
//!
//! - [`init`] installs the `tracing` subscriber (JSON logs with `trace_id`/`span_id`,
//!   spans → OTEL spans, events → OTLP logs). Export is on when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.
//! - [`OtelInterceptor`] opens an `rpc.server` span per call, continuing the caller's `traceparent`.
//! - [`TracingLogs`], [`OtelMetrics`]: backends for the services' injected `Logger` / `Metrics`.
//! - [`TracingTransport`] wraps a client transport with an `rpc.client` span and injects `traceparent`.

mod client;
mod init;
mod interceptor;
mod log_backend;
mod logs;
mod metrics_backend;
mod propagation;
#[cfg(test)]
mod test_meter;

pub use client::TracingTransport;
pub use init::{TelemetryGuard, init};
pub use interceptor::{OtelInterceptor, RequestContextOtel};
pub use log_backend::TracingLogs;
pub use logs::JsonLogFormat;
pub use metrics_backend::{OtelMetrics, SECONDS_BUCKETS};
pub use propagation::{
    PROPAGATION_HEADERS, continue_trace, extract_http, extract_nats, inject_http, inject_nats, is_propagation_header,
    split_procedure,
};

use std::time::Instant;

/// Milliseconds since `started`, rounded to µs.
pub(crate) fn elapsed_ms(started: Instant) -> f64 {
    (started.elapsed().as_secs_f64() * 1_000_000.0).round() / 1000.0
}
