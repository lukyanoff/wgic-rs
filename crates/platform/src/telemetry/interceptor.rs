//! Server side: one `rpc.server` span per call, on every protocol and transport.

use std::time::Instant;

use connectrpc::interceptor::{
    Interceptor, Next, NextStream, PayloadStream, StreamRequest, StreamResponse, UnaryRequest,
    UnaryResponse,
};
use connectrpc::{ConnectError, Protocol, RequestContext};
use futures::StreamExt;
use kernel::{RequestContextExt, Transport};
use opentelemetry::Context;
use opentelemetry::trace::TraceContextExt;
use tracing::field::Empty;
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{continue_trace, elapsed_ms, extract_http, split_procedure};

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
        self.extensions()
            .get::<Context>()
            .cloned()
            .unwrap_or_else(Context::current)
    }
    fn trace_id(&self) -> Option<String> {
        let span_context = self.otel().span().span_context().clone();
        span_context
            .is_valid()
            .then(|| span_context.trace_id().to_string())
    }
}

/// Opens an `rpc.server` span for every call, continuing the caller's trace.
/// Register with `ConnectRpcService::with_interceptor(OtelInterceptor)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct OtelInterceptor;

#[connectrpc::async_trait]
impl Interceptor for OtelInterceptor {
    async fn intercept_unary(
        &self,
        mut req: UnaryRequest,
        next: Next<'_>,
    ) -> Result<UnaryResponse, ConnectError> {
        let span = server_span(&mut req.ctx);
        let started = Instant::now();
        let result = next.run(req).instrument(span.clone()).await;
        finish_server(&span, started, result.as_ref().err(), None);
        result
    }

    async fn intercept_streaming(
        &self,
        mut req: StreamRequest,
        inbound: PayloadStream,
        next: NextStream<'_>,
    ) -> Result<StreamResponse, ConnectError> {
        let span = server_span(&mut req.ctx);
        let started = Instant::now();
        let result = next.run(req, inbound).instrument(span.clone()).await;
        match result {
            Ok(response) => {
                Ok(response.map_body(move |stream| finish_on_end(stream, span, started)))
            }
            Err(e) => {
                finish_server(&span, started, Some(&e), None);
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

fn server_span(ctx: &mut RequestContext) -> Span {
    let procedure = ctx
        .path()
        .map(str::to_string)
        .or_else(|| ctx.spec().map(|s| s.procedure.to_string()))
        .unwrap_or_else(|| "unknown".into());
    let (service, method) = split_procedure(&procedure);
    let (system, wire) = protocol_names(ctx.protocol());
    let content_type = ctx
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let transport = ctx.transport();
    let subject = match &transport {
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

fn finish_server(span: &Span, started: Instant, err: Option<&ConnectError>, messages: Option<u64>) {
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
    span: Span,
    started: Instant,
    count: u64,
    error: Option<ConnectError>,
}

/// Poll the outbound stream inside the span and close the span when the stream ends.
fn finish_on_end(stream: PayloadStream, span: Span, started: Instant) -> PayloadStream {
    let state = StreamState {
        inner: stream,
        span,
        started,
        count: 0,
        error: None,
    };
    Box::pin(futures::stream::unfold(Some(state), |state| async move {
        let mut state = state?;
        let Some(item) = state.inner.next().instrument(state.span.clone()).await else {
            finish_server(
                &state.span,
                state.started,
                state.error.as_ref(),
                Some(state.count),
            );
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
