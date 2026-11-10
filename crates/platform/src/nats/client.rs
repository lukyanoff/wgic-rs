//! Call connect services over NATS request/reply with any generated client.

use std::time::Duration;

use async_nats::{HeaderMap, HeaderName as NatsHeaderName, HeaderValue as NatsHeaderValue};
use bytes::Bytes;
use connectrpc::client::{ClientBody, ClientTransport};
use connectrpc::{ConnectError, ErrorCode};
use futures::future::BoxFuture;
use http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use tracing::field::Empty;
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::wire::{CONNECT_CODE, NATS_SERVICE_ERROR, NATS_SERVICE_ERROR_CODE, code_for_status, subject_for};
use crate::telemetry::{continue_trace, extract_http, inject_nats, is_propagation_header};

/// `ClientTransport` that sends Connect unary calls over NATS request/reply:
///
/// ```ignore
/// let client = GreetServiceClient::new(NatsTransport::new(nc, "rpc"), ClientConfig::new("http://nats".parse()?));
/// ```
#[derive(Clone)]
pub struct NatsTransport {
    nc: async_nats::Client,
    prefix: String,
}

impl NatsTransport {
    pub fn new(nc: async_nats::Client, prefix: impl Into<String>) -> Self {
        Self { nc, prefix: prefix.into() }
    }
}

impl ClientTransport for NatsTransport {
    type ResponseBody = Full<Bytes>;
    type Error = ConnectError;

    fn send(&self, request: Request<ClientBody>) -> BoxFuture<'static, Result<Response<Full<Bytes>>, ConnectError>> {
        let nc = self.nc.clone();
        let (parts, body) = request.into_parts();
        let subject = subject_for(&self.prefix, parts.uri.path());
        let span = request_span(&subject);
        // Parent: the caller's context from the request headers (e.g. TracingTransport),
        // otherwise the current span.
        continue_trace(&span, extract_http(&parts.headers));
        let call = async move {
            let payload = body.collect().await?.to_bytes();
            let request = async_nats::Request::new()
                .payload(payload)
                .headers(nats_headers(&parts.headers))
                .timeout(timeout_from(&parts.headers));
            let reply = nc.send_request(subject.clone(), request).await.map_err(|e| request_failed(&subject, e))?;
            record_service_error(&reply);
            Ok(reply_to_response(reply))
        };
        Box::pin(call.instrument(span))
    }
}

fn request_span(subject: &str) -> Span {
    tracing::info_span!(
        "nats.request",
        otel.name = format!("send {subject}").as_str(),
        otel.kind = "producer",
        otel.status_code = Empty,
        otel.status_message = Empty,
        messaging.system = "nats",
        messaging.operation.type = "send",
        messaging.destination.name = subject,
        messaging.nats.service_error_code = Empty,
    )
}

/// Copy RPC headers, replacing trace propagation with the current span's context.
fn nats_headers(http_headers: &http::HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in http_headers.iter() {
        if is_propagation_header(name.as_str()) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            headers.append(name.as_str(), v);
        }
    }
    inject_nats(&Span::current().context(), &mut headers);
    headers
}

/// The Connect deadline (`connect-timeout-ms`) as a NATS request timeout.
fn timeout_from(headers: &http::HeaderMap) -> Option<Duration> {
    let millis = headers.get("connect-timeout-ms")?.to_str().ok()?;
    millis.parse::<u64>().ok().map(Duration::from_millis)
}

fn request_failed(subject: &str, e: async_nats::RequestError) -> ConnectError {
    use async_nats::RequestErrorKind::*;
    let error = match e.kind() {
        NoResponders => ConnectError::new(ErrorCode::Unavailable, format!("no NATS responders for {subject}")),
        TimedOut => ConnectError::new(ErrorCode::DeadlineExceeded, format!("NATS request to {subject} timed out")),
        _ => ConnectError::new(ErrorCode::Unavailable, format!("NATS {subject}: {e}")),
    };
    let span = Span::current();
    span.record("otel.status_code", "ERROR");
    span.record("otel.status_message", error.message.as_deref().unwrap_or(""));
    tracing::warn!(%subject, code = error.code.as_str(), "nats request failed");
    error
}

fn record_service_error(reply: &async_nats::Message) {
    let Some(code) = reply.headers.as_ref().and_then(|h| h.get(NATS_SERVICE_ERROR_CODE)) else {
        return;
    };
    let span = Span::current();
    span.record("otel.status_code", "ERROR");
    span.record("messaging.nats.service_error_code", code.as_str().parse::<u64>().unwrap_or(0));
}

/// NATS reply → HTTP response the connect client understands.
fn reply_to_response(reply: async_nats::Message) -> Response<Full<Bytes>> {
    let headers = reply.headers.unwrap_or_default();
    if let Some(status) = headers.get(NATS_SERVICE_ERROR_CODE) {
        return error_response(&headers, status.as_str());
    }
    let mut response = Response::new(Full::new(reply.payload));
    for (name, values) in headers.iter() {
        let name: &NatsHeaderName = name;
        let Ok(name) = HeaderName::from_bytes(AsRef::<str>::as_ref(name).as_bytes()) else {
            continue;
        };
        for value in values {
            let value: &NatsHeaderValue = value;
            if let Ok(v) = HeaderValue::from_str(value.as_str()) {
                response.headers_mut().append(name.clone(), v);
            }
        }
    }
    response
}

/// A NATS micro error reply as a Connect unary error response.
fn error_response(headers: &HeaderMap, status: &str) -> Response<Full<Bytes>> {
    let status = status.parse::<u16>().unwrap_or(500);
    let get = |key: &str| headers.get(key).map(|v| v.as_str().to_string());
    let code = get(CONNECT_CODE).unwrap_or_else(|| code_for_status(status).to_string());
    let message = get(NATS_SERVICE_ERROR).unwrap_or_default();
    let body = serde_json::json!({ "code": code, "message": message }).to_string();
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(http::header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
