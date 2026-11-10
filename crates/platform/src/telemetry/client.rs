//! Client side: one `rpc.client` span per outgoing call.

use std::time::Instant;

use connectrpc::client::{ClientBody, ClientTransport};
use futures::future::BoxFuture;
use http::{Request, Response};
use tracing::field::Empty;
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{PROPAGATION_HEADERS, elapsed_ms, inject_http, split_procedure};

/// Wraps any [`ClientTransport`] (HTTP or NATS) with an `rpc.client` span and
/// injects `traceparent` into the request.
#[derive(Clone)]
pub struct TracingTransport<T>(pub T);

impl<T: ClientTransport> ClientTransport for TracingTransport<T> {
    type ResponseBody = T::ResponseBody;
    type Error = T::Error;

    fn send(
        &self,
        mut request: Request<ClientBody>,
    ) -> BoxFuture<'static, Result<Response<Self::ResponseBody>, Self::Error>> {
        let span = client_span(&request);
        for header in PROPAGATION_HEADERS {
            request.headers_mut().remove(header);
        }
        inject_http(&span.context(), request.headers_mut());
        let pending = span.in_scope(|| self.0.send(request));
        Box::pin(async move {
            let started = Instant::now();
            let result = pending.instrument(span.clone()).await;
            record_outcome(&span, started, &result);
            result
        })
    }
}

fn client_span(request: &Request<ClientBody>) -> Span {
    let path = request.uri().path();
    let (service, method) = split_procedure(path);
    tracing::info_span!(
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
    )
}

fn record_outcome<B, E: std::fmt::Display>(span: &Span, started: Instant, result: &Result<Response<B>, E>) {
    let duration_ms = elapsed_ms(started);
    let _entered = span.enter();
    match result {
        Ok(response) => record_status(span, duration_ms, response.status()),
        Err(e) => {
            span.record("otel.status_code", "ERROR");
            span.record("otel.status_message", e.to_string().as_str());
            tracing::warn!(duration_ms, error = %e, "rpc transport error");
        }
    }
}

fn record_status(span: &Span, duration_ms: f64, status: http::StatusCode) {
    span.record("http.response.status_code", status.as_u16());
    if status.is_success() {
        tracing::debug!(duration_ms, status = status.as_u16(), "rpc response");
        return;
    }
    span.record("otel.status_code", "ERROR");
    tracing::warn!(duration_ms, status = status.as_u16(), "rpc error response");
}
