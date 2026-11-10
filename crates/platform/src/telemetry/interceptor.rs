//! OpenTelemetry for every RPC, on every protocol and transport, and structured JSON logs
//! correlated with the active trace.
//!
//! - [`init`] installs the `tracing` subscriber: JSON logs on stdout (with `trace_id`/`span_id`),
//!   `tracing` spans → OTEL spans, `tracing` events → OTEL span events + OTLP log records.
//!   OTLP export is on when `OTEL_EXPORTER_OTLP_ENDPOINT` (or a per-signal variant) is set.
//! - [`OtelInterceptor`] opens an `rpc.server` span per call (Connect, gRPC, gRPC-web over
//!   HTTP, and Connect over NATS), continuing the caller's W3C `traceparent`, and puts the
//!   span's [`opentelemetry::Context`] into the [`RequestContext`] (see [`RequestContextOtel`]).
//! - [`TracingTransport`] wraps any client transport with an `rpc.client` span and injects
//!   `traceparent` into the outgoing request.

use std::fmt;
use std::time::Instant;

use connectrpc::client::{ClientBody, ClientTransport};
use connectrpc::interceptor::{
    Interceptor, Next, NextStream, PayloadStream, StreamRequest, StreamResponse, UnaryRequest,
    UnaryResponse,
};
use connectrpc::{ConnectError, Protocol, RequestContext};
use futures::StreamExt;
use futures::future::BoxFuture;
use http::{HeaderMap, Request, Response};
use opentelemetry::propagation::{
    Extractor, Injector, TextMapCompositePropagator,
};
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry::{Context, KeyValue, global};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_http::{HeaderExtractor, HeaderInjector};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::propagation::{BaggagePropagator, TraceContextPropagator};
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde_json::{Map, Value};
use tracing::field::{Empty, Field, Visit};
use tracing::{Instrument, Span, Subscriber};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::filter::{EnvFilter, filter_fn};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt as tfmt};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

/// Flushes and shuts down the OTEL providers when dropped. Keep it alive for the
/// whole `main`.
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

fn env_set(k: &str) -> bool {
    std::env::var(k).is_ok_and(|v| !v.trim().is_empty())
}

/// Install tracing + OpenTelemetry for this process.
///
/// - `OTEL_SERVICE_NAME` overrides `default_service`.
/// - `OTEL_EXPORTER_OTLP_ENDPOINT` (e.g. `http://127.0.0.1:4318`) turns on OTLP/HTTP export of
///   traces and logs. Without it, spans still get real IDs, so logs stay correlated.
/// - `RUST_LOG` sets the log filter (default `info`).
pub fn init(default_service: &str) -> TelemetryGuard {
    let service = std::env::var("OTEL_SERVICE_NAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_service.to_string());
    let resource = Resource::builder_empty()
        .with_service_name(service)
        .with_attributes([
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("service.instance.id", uuid::Uuid::new_v4().to_string()),
        ])
        .build();

    global::set_text_map_propagator(TextMapCompositePropagator::new(vec![
        Box::new(TraceContextPropagator::new()),
        Box::new(BaggagePropagator::new()),
    ]));

    let mut traces = SdkTracerProvider::builder().with_resource(resource.clone());
    let mut logs = SdkLoggerProvider::builder().with_resource(resource);
    let export_traces =
        env_set("OTEL_EXPORTER_OTLP_ENDPOINT") || env_set("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
    let export_logs =
        env_set("OTEL_EXPORTER_OTLP_ENDPOINT") || env_set("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT");
    if export_traces {
        match opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .build()
        {
            Ok(e) => traces = traces.with_batch_exporter(e),
            Err(e) => eprintln!("otel: cannot build span exporter: {e}"),
        }
    }
    if export_logs {
        match opentelemetry_otlp::LogExporter::builder().with_http().build() {
            Ok(e) => logs = logs.with_batch_exporter(e),
            Err(e) => eprintln!("otel: cannot build log exporter: {e}"),
        }
    }
    let tracer_provider = traces.build();
    let logger_provider = logs.build();
    global::set_tracer_provider(tracer_provider.clone());

    // Never feed the exporters' own diagnostics back into the exporters.
    let not_otel_internals = || {
        filter_fn(|m| {
            let t = m.target();
            !(t.starts_with("opentelemetry")
                || t.starts_with("hyper")
                || t.starts_with("reqwest")
                || t.starts_with("h2"))
        })
    };

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer_provider.tracer(env!("CARGO_PKG_NAME")))
                .with_error_events_to_status(true),
        )
        .with(OpenTelemetryTracingBridge::new(&logger_provider).with_filter(not_otel_internals()))
        .with(tfmt::layer().event_format(JsonLogFormat))
        .init();

    tracing::debug!(
        traces = export_traces,
        logs = export_logs,
        "telemetry initialized"
    );
    TelemetryGuard {
        tracer_provider,
        logger_provider,
    }
}

// ---------------------------------------------------------------------------
// JSON logs
// ---------------------------------------------------------------------------

/// One JSON object per line:
/// `{"timestamp","level","target","message",...fields,"span","trace_id","span_id"}`.
pub struct JsonLogFormat;

#[derive(Default)]
struct JsonFields(Map<String, Value>);

impl Visit for JsonFields {
    fn record_str(&mut self, f: &Field, v: &str) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_i64(&mut self, f: &Field, v: i64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_u64(&mut self, f: &Field, v: u64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_f64(&mut self, f: &Field, v: f64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_bool(&mut self, f: &Field, v: bool) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_error(&mut self, f: &Field, v: &(dyn std::error::Error + 'static)) {
        self.0.insert(f.name().into(), v.to_string().into());
    }
    fn record_debug(&mut self, f: &Field, v: &dyn fmt::Debug) {
        self.0.insert(f.name().into(), format!("{v:?}").into());
    }
}

impl<S, N> FormatEvent<S, N> for JsonLogFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let mut ts = String::new();
        SystemTime.format_time(&mut Writer::new(&mut ts))?;

        let mut fields = JsonFields::default();
        event.record(&mut fields);

        let mut out = Map::new();
        out.insert("timestamp".into(), ts.into());
        out.insert("level".into(), meta.level().as_str().into());
        out.insert("target".into(), meta.target().into());
        if let Some(m) = fields.0.remove("message") {
            out.insert("message".into(), m);
        }
        out.extend(fields.0);

        if let Some(span) = ctx.lookup_current() {
            out.insert("span".into(), span.name().into());
        }
        let cx = Span::current().context();
        let sc = cx.span().span_context().clone();
        if sc.is_valid() {
            out.insert("trace_id".into(), sc.trace_id().to_string().into());
            out.insert("span_id".into(), sc.span_id().to_string().into());
        }
        let line = serde_json::to_string(&Value::Object(out)).map_err(|_| fmt::Error)?;
        writeln!(writer, "{line}")
    }
}

// ---------------------------------------------------------------------------
// Propagation helpers
// ---------------------------------------------------------------------------

/// The W3C propagation headers we own; they are rewritten at every hop.
pub const PROPAGATION_HEADERS: [&str; 3] = ["traceparent", "tracestate", "baggage"];

pub fn is_propagation_header(name: &str) -> bool {
    PROPAGATION_HEADERS
        .iter()
        .any(|h| h.eq_ignore_ascii_case(name))
}

/// Parent context from HTTP headers (`traceparent`, `tracestate`, `baggage`).
pub fn extract_http(headers: &HeaderMap) -> Context {
    global::get_text_map_propagator(|p| p.extract(&HeaderExtractor(headers)))
}

/// Write `cx` into HTTP headers.
pub fn inject_http(cx: &Context, headers: &mut HeaderMap) {
    global::get_text_map_propagator(|p| p.inject_context(cx, &mut HeaderInjector(headers)));
}

/// Read propagation headers from a NATS message (case-insensitive).
pub struct NatsHeaderExtractor<'a>(pub &'a async_nats::HeaderMap);

impl Extractor for NatsHeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| AsRef::<str>::as_ref(*k).eq_ignore_ascii_case(key))
            .and_then(|(_, vs)| vs.first())
            .map(|v| v.as_str())
    }
    fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(k, _)| AsRef::<str>::as_ref(k)).collect()
    }
}

/// Write propagation headers into a NATS message.
pub struct NatsHeaderInjector<'a>(pub &'a mut async_nats::HeaderMap);

impl Injector for NatsHeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key, value.as_str());
    }
}

pub fn extract_nats(headers: Option<&async_nats::HeaderMap>) -> Context {
    match headers {
        Some(h) => global::get_text_map_propagator(|p| p.extract(&NatsHeaderExtractor(h))),
        None => Context::new(),
    }
}

pub fn inject_nats(cx: &Context, headers: &mut async_nats::HeaderMap) {
    global::get_text_map_propagator(|p| p.inject_context(cx, &mut NatsHeaderInjector(headers)));
}

/// Make `parent` the span's parent if it carries a valid remote span context.
pub fn continue_trace(span: &Span, parent: Context) {
    if parent.span().span_context().is_valid() {
        let _ = span.set_parent(parent);
    }
}

/// `/greet.v1.GreetService/Greet` → (`greet.v1.GreetService`, `Greet`)
pub fn split_procedure(path: &str) -> (&str, &str) {
    let p = path.trim_start_matches('/');
    p.rsplit_once('/').unwrap_or(("", p))
}

// ---------------------------------------------------------------------------
// Request context
// ---------------------------------------------------------------------------

/// How a request reached the connect service. The NATS adapter sets
/// [`Transport::Nats`] as a request extension; absent means HTTP.
#[derive(Debug, Clone)]
pub enum Transport {
    Http,
    Nats { subject: String },
}

impl Transport {
    /// `http` or `nats`.
    pub fn name(&self) -> &'static str {
        match self {
            Transport::Http => "http",
            Transport::Nats { .. } => "nats",
        }
    }
}

/// OTEL access for handlers: `ctx.otel()`, `ctx.trace_id()`.
pub trait RequestContextOtel {
    /// The `rpc.server` span's OTEL context (falls back to the current one).
    fn otel(&self) -> Context;
    /// Hex trace id of this call, if it is traced.
    fn trace_id(&self) -> Option<String>;
    /// How the request arrived.
    fn transport(&self) -> Transport;
}

impl RequestContextOtel for RequestContext {
    fn otel(&self) -> Context {
        self.extensions()
            .get::<Context>()
            .cloned()
            .unwrap_or_else(Context::current)
    }
    fn trace_id(&self) -> Option<String> {
        let cx = self.otel();
        let sc = cx.span().span_context().clone();
        sc.is_valid().then(|| sc.trace_id().to_string())
    }
    fn transport(&self) -> Transport {
        self.extensions()
            .get::<Transport>()
            .cloned()
            .unwrap_or(Transport::Http)
    }
}

// ---------------------------------------------------------------------------
// Server interceptor
// ---------------------------------------------------------------------------

/// Opens an `rpc.server` span for every call, continuing the caller's trace.
/// Register with `ConnectRpcService::with_interceptor(OtelInterceptor)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct OtelInterceptor;

fn server_span(ctx: &mut RequestContext) -> Span {
    let procedure = ctx
        .path()
        .map(str::to_string)
        .or_else(|| ctx.spec().map(|s| s.procedure.to_string()))
        .unwrap_or_else(|| "unknown".into());
    let (service, method) = split_procedure(&procedure);
    let (system, wire) = match ctx.protocol() {
        Some(Protocol::Connect) => ("connect_rpc", "connect"),
        Some(Protocol::Grpc) => ("grpc", "grpc"),
        Some(Protocol::GrpcWeb) => ("grpc", "grpc_web"),
        _ => ("connect_rpc", "unknown"),
    };
    let content_type = ctx
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let transport = ctx.transport();
    let transport_name = transport.name();
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
        rpc.transport = transport_name,
        rpc.content_type = content_type.as_str(),
        messaging.destination.name = subject,
        rpc.connect_rpc.error_code = Empty,
    );
    continue_trace(&span, extract_http(ctx.headers()));
    ctx.extensions_mut().insert(span.context());
    span
}

/// Milliseconds since `started`, rounded to µs.
fn elapsed_ms(started: Instant) -> f64 {
    (started.elapsed().as_secs_f64() * 1_000_000.0).round() / 1000.0
}

fn finish_server(span: &Span, started: Instant, err: Option<&ConnectError>, messages: Option<u64>) {
    let duration_ms = elapsed_ms(started);
    let _e = span.enter();
    match err {
        None => tracing::info!(duration_ms, code = "ok", messages, "rpc finished"),
        Some(e) => {
            let code = e.code.as_str();
            let message = e.message.clone().unwrap_or_default();
            span.record("rpc.connect_rpc.error_code", code);
            span.record("otel.status_code", "ERROR");
            span.record("otel.status_message", message.as_str());
            tracing::warn!(duration_ms, code, error = %message, messages, "rpc failed");
        }
    }
}

#[connectrpc::async_trait]
impl Interceptor for OtelInterceptor {
    async fn intercept_unary(
        &self,
        mut req: UnaryRequest,
        next: Next<'_>,
    ) -> Result<UnaryResponse, ConnectError> {
        let span = server_span(&mut req.ctx);
        let started = Instant::now();
        let res = next.run(req).instrument(span.clone()).await;
        finish_server(&span, started, res.as_ref().err(), None);
        res
    }

    async fn intercept_streaming(
        &self,
        mut req: StreamRequest,
        inbound: PayloadStream,
        next: NextStream<'_>,
    ) -> Result<StreamResponse, ConnectError> {
        let span = server_span(&mut req.ctx);
        let started = Instant::now();
        let res = next.run(req, inbound).instrument(span.clone()).await;
        let resp = match res {
            Ok(r) => r,
            Err(e) => {
                finish_server(&span, started, Some(&e), None);
                return Err(e);
            }
        };
        // Poll the outbound stream inside the span and close the span when it ends.
        Ok(resp.map_body(move |stream| -> PayloadStream {
            struct St {
                inner: PayloadStream,
                span: Span,
                started: Instant,
                count: u64,
                error: Option<ConnectError>,
            }
            let st = St {
                inner: stream,
                span,
                started,
                count: 0,
                error: None,
            };
            Box::pin(futures::stream::unfold(Some(st), |st| async move {
                let mut st = st?;
                match st.inner.next().instrument(st.span.clone()).await {
                    Some(item) => {
                        match &item {
                            Ok(_) => st.count += 1,
                            Err(e) => st.error = Some(e.clone()),
                        }
                        Some((item, Some(st)))
                    }
                    None => {
                        finish_server(&st.span, st.started, st.error.as_ref(), Some(st.count));
                        None
                    }
                }
            }))
        }))
    }
}

// ---------------------------------------------------------------------------
// Client transport
// ---------------------------------------------------------------------------

/// Wraps any [`ClientTransport`] (HTTP or [`crate::NatsTransport`]) with an
/// `rpc.client` span and injects `traceparent` into the request.
#[derive(Clone)]
pub struct TracingTransport<T>(pub T);

impl<T: ClientTransport> ClientTransport for TracingTransport<T> {
    type ResponseBody = T::ResponseBody;
    type Error = T::Error;

    fn send(
        &self,
        mut request: Request<ClientBody>,
    ) -> BoxFuture<'static, Result<Response<Self::ResponseBody>, Self::Error>> {
        let path = request.uri().path().to_string();
        let (service, method) = split_procedure(&path);
        let span = tracing::info_span!(
            "rpc.client",
            otel.name = path.trim_start_matches('/'),
            otel.kind = "client",
            otel.status_code = Empty,
            otel.status_message = Empty,
            rpc.system = "connect_rpc",
            rpc.service = service,
            rpc.method = method,
            server.address = request.uri().host().unwrap_or(""),
            http.response.status_code = Empty,
        );
        for h in PROPAGATION_HEADERS {
            request.headers_mut().remove(h);
        }
        inject_http(&span.context(), request.headers_mut());
        let fut = span.in_scope(|| self.0.send(request));
        Box::pin(async move {
            let started = Instant::now();
            let res = fut.instrument(span.clone()).await;
            let duration_ms = elapsed_ms(started);
            let _e = span.enter();
            match &res {
                Ok(r) => {
                    let status = r.status().as_u16();
                    span.record("http.response.status_code", status);
                    if r.status().is_success() {
                        tracing::debug!(duration_ms, status, "rpc response");
                    } else {
                        span.record("otel.status_code", "ERROR");
                        tracing::warn!(duration_ms, status, "rpc error response");
                    }
                }
                Err(e) => {
                    span.record("otel.status_code", "ERROR");
                    span.record("otel.status_message", e.to_string().as_str());
                    tracing::warn!(duration_ms, error = %e, "rpc transport error");
                }
            }
            drop(_e);
            res
        })
    }
}
