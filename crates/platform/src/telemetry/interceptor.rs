//! Server side: one `rpc.server` span and one `rpc.server.call.duration` measurement per call,
//! on every protocol and transport.

use std::time::Instant;

use connectrpc::interceptor::{
    Interceptor, Next, NextStream, PayloadStream, StreamRequest, StreamResponse, UnaryRequest, UnaryResponse,
};
use connectrpc::{ConnectError, Protocol, RequestContext};
use futures::StreamExt;
use kernel::{RequestContextExt, Transport};
use opentelemetry::metrics::Histogram;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::{Context, KeyValue, global};
use tracing::field::Empty;
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{SECONDS_BUCKETS, continue_trace, elapsed_ms, extract_http, split_procedure};

/// OTEL access for handlers that need it: `ctx.otel()`, `ctx.trace_id()`.
/// Most handlers don't: logs inside a handler already carry `trace_id`.
pub trait RequestContextOtel {
    /// The `rpc.server` span's OTEL context (falls back to the current one).
    fn otel(&self) -> Context;
    /// Hex trace id of this call, if it is traced.
    fn trace_id(&self) -> Option<String>;
}

impl RequestContextOtel for RequestContext {
    fn otel(&self) -> Context {
        self.extensions().get::<Context>().cloned().unwrap_or_else(Context::current)
    }
    fn trace_id(&self) -> Option<String> {
        let span_context = self.otel().span().span_context().clone();
        span_context.is_valid().then(|| span_context.trace_id().to_string())
    }
}

/// Opens an `rpc.server` span for every call, continuing the caller's trace, and records
/// the call duration in the `rpc.server.call.duration` histogram (seconds).
/// Register with `ConnectRpcService::with_interceptor(OtelInterceptor::new())`.
#[derive(Clone)]
pub struct OtelInterceptor {
    duration: Histogram<f64>,
}

impl OtelInterceptor {
    /// Uses the global meter provider, so create it after `telemetry::init`.
    pub fn new() -> Self {
        let meter = global::meter("platform");
        let duration = meter
            .f64_histogram("rpc.server.call.duration")
            .with_unit("s")
            .with_description("Duration of RPC calls handled by this server")
            .with_boundaries(SECONDS_BUCKETS.to_vec())
            .build();
        Self { duration }
    }
}

impl Default for OtelInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

#[connectrpc::async_trait]
impl Interceptor for OtelInterceptor {
    async fn intercept_unary(&self, mut req: UnaryRequest, next: Next<'_>) -> Result<UnaryResponse, ConnectError> {
        let call = Call::start(&mut req.ctx, self.duration.clone());
        let result = next.run(req).instrument(call.span.clone()).await;
        call.finish(result.as_ref().err(), None);
        result
    }

    async fn intercept_streaming(
        &self,
        mut req: StreamRequest,
        inbound: PayloadStream,
        next: NextStream<'_>,
    ) -> Result<StreamResponse, ConnectError> {
        let call = Call::start(&mut req.ctx, self.duration.clone());
        let result = next.run(req, inbound).instrument(call.span.clone()).await;
        match result {
            Ok(response) => Ok(response.map_body(move |stream| finish_on_end(stream, call))),
            Err(e) => {
                call.finish(Some(&e), None);
                Err(e)
            }
        }
    }
}

/// Wire protocol as OTEL semantic-convention values: (`rpc.system`, wire name).
fn protocol_names(protocol: Option<Protocol>) -> (&'static str, &'static str) {
    match protocol {
        Some(Protocol::Connect) => ("connect_rpc", "connect"),
        Some(Protocol::Grpc) => ("grpc", "grpc"),
        Some(Protocol::GrpcWeb) => ("grpc", "grpc_web"),
        _ => ("connect_rpc", "unknown"),
    }
}

/// One call in progress: its span, start time and metric attributes.
struct Call {
    span: Span,
    started: Instant,
    /// Low-cardinality attributes for the duration metric (no subject, no content type).
    attributes: Vec<KeyValue>,
    duration: Histogram<f64>,
}

impl Call {
    fn start(ctx: &mut RequestContext, duration: Histogram<f64>) -> Call {
        let procedure = ctx
            .path()
            .map(str::to_string)
            .or_else(|| ctx.spec().map(|s| s.procedure.to_string()))
            .unwrap_or_else(|| "unknown".into());
        let (service, method) = split_procedure(&procedure);
        let (system, wire) = protocol_names(ctx.protocol());
        let transport = ctx.transport();
        let span = server_span(ctx, &procedure, (system, wire), &transport);
        let attributes = vec![
            KeyValue::new("rpc.system", system),
            KeyValue::new("rpc.service", service.to_string()),
            KeyValue::new("rpc.method", method.to_string()),
            KeyValue::new("rpc.transport", transport.name()),
        ];
        Call { span, started: Instant::now(), attributes, duration }
    }

    /// Close the call: log + span status + duration metric.
    fn finish(&self, err: Option<&ConnectError>, messages: Option<u64>) {
        let code = err.map_or("ok", |e| e.code.as_str());
        let mut attributes = self.attributes.clone();
        attributes.push(KeyValue::new("rpc.response.status_code", code));
        self.duration.record(self.started.elapsed().as_secs_f64(), &attributes);
        finish_span(&self.span, self.started, err, messages);
    }
}

fn server_span(ctx: &mut RequestContext, procedure: &str, (system, wire): (&str, &str), transport: &Transport) -> Span {
    let (service, method) = split_procedure(procedure);
    let content_type =
        ctx.headers().get(http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let subject = match transport {
        Transport::Http => None,
        Transport::Nats { subject } => Some(subject.as_str()),
    };
    let span = tracing::info_span!(
        "rpc.server",
        otel.name = procedure.trim_start_matches('/'),
        otel.kind = "server",
        otel.status_code = Empty,
        otel.status_message = Empty,
        rpc.system = system,
        rpc.service = service,
        rpc.method = method,
        rpc.wire_protocol = wire,
        rpc.transport = transport.name(),
        rpc.content_type = content_type.as_str(),
        messaging.destination.name = subject,
        rpc.connect_rpc.error_code = Empty,
    );
    continue_trace(&span, extract_http(ctx.headers()));
    ctx.extensions_mut().insert(span.context());
    span
}

fn finish_span(span: &Span, started: Instant, err: Option<&ConnectError>, messages: Option<u64>) {
    let duration_ms = elapsed_ms(started);
    let _entered = span.enter();
    let Some(e) = err else {
        tracing::info!(duration_ms, code = "ok", messages, "rpc finished");
        return;
    };
    let code = e.code.as_str();
    let message = e.message.clone().unwrap_or_default();
    span.record("rpc.connect_rpc.error_code", code);
    span.record("otel.status_code", "ERROR");
    span.record("otel.status_message", message.as_str());
    tracing::warn!(duration_ms, code, error = %message, messages, "rpc failed");
}

/// State carried while a server stream is being sent.
struct StreamState {
    inner: PayloadStream,
    call: Call,
    count: u64,
    error: Option<ConnectError>,
}

/// Poll the outbound stream inside the span and finish the call when the stream ends.
fn finish_on_end(stream: PayloadStream, call: Call) -> PayloadStream {
    let state = StreamState { inner: stream, call, count: 0, error: None };
    Box::pin(futures::stream::unfold(Some(state), |state| async move {
        let mut state = state?;
        let next = state.inner.next().instrument(state.call.span.clone()).await;
        let Some(item) = next else {
            state.call.finish(state.error.as_ref(), Some(state.count));
            return None;
        };
        match &item {
            Ok(_) => state.count += 1,
            Err(e) => state.error = Some(e.clone()),
        }
        Some((item, Some(state)))
    }))
}

#[cfg(test)]
#[path = "interceptor_tests.rs"]
mod tests;
