//! Host connect services as NATS micro endpoints.

use std::collections::{HashMap, HashSet};

use anyhow::anyhow;
use async_nats::HeaderMap;
use async_nats::service::endpoint::Endpoint;
use async_nats::service::{Request as NatsRequest, Service, ServiceExt};
use bytes::Bytes;
use connectrpc::ConnectRpcService;
use futures::StreamExt;
use http::{HeaderName, HeaderValue, Request, StatusCode, response};
use http_body_util::{BodyExt, Full};
use kernel::Transport;
use tower::ServiceExt as _;
use tracing::field::Empty;
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::wire::{CONNECT_CODE, NatsConfig, endpoint_name, parse_connect_error, subject_for};
use crate::telemetry::{continue_trace, extract_nats, inject_http, is_propagation_header};

type Reply = (Result<Bytes, async_nats::service::error::Error>, HeaderMap);

/// Host every method of `service` as a NATS micro endpoint of the micro service `name`.
/// Runs until the connection closes. All instances with the same `name` share a queue
/// group, so NATS load-balances requests across them.
pub async fn serve_nats(
    service: ConnectRpcService,
    methods: Vec<String>,
    nc: async_nats::Client,
    cfg: &NatsConfig,
    name: &str,
) -> anyhow::Result<()> {
    let micro = nc
        .service_builder()
        .description(format!("{name}: connectrpc over NATS micro"))
        .start(name, &cfg.version)
        .await
        .map_err(|e| anyhow!("start NATS micro service '{name}': {e}"))?;

    let mut taken = HashSet::new();
    let mut loops = Vec::new();
    for path in methods {
        let rpc_path = format!("/{}", path.trim_start_matches('/'));
        let endpoint = add_endpoint(&micro, cfg, &rpc_path, &mut taken).await?;
        loops.push(tokio::spawn(serve_endpoint(endpoint, service.clone(), rpc_path)));
    }
    tracing::info!(service = %name, version = %cfg.version, url = %cfg.url, "nats: micro service started");
    for task in loops {
        task.await?;
    }
    Ok(())
}

async fn add_endpoint(
    micro: &Service,
    cfg: &NatsConfig,
    rpc_path: &str,
    taken: &mut HashSet<String>,
) -> anyhow::Result<Endpoint> {
    let subject = subject_for(&cfg.prefix, rpc_path);
    let name = endpoint_name(rpc_path, taken);
    let endpoint = micro
        .endpoint_builder()
        .name(&name)
        .metadata(HashMap::from([("rpc".to_string(), rpc_path.to_string())]))
        .add(&subject)
        .await
        .map_err(|e| anyhow!("add NATS endpoint '{name}' on {subject}: {e}"))?;
    tracing::info!(endpoint = %name, %subject, "nats: endpoint added");
    Ok(endpoint)
}

/// Answer requests for one endpoint, each in its own task.
async fn serve_endpoint(mut endpoint: Endpoint, service: ConnectRpcService, rpc_path: String) {
    while let Some(request) = endpoint.next().await {
        let span = receive_span(&request.message);
        let task = respond(request, service.clone(), rpc_path.clone());
        tokio::spawn(task.instrument(span));
    }
}

async fn respond(request: NatsRequest, service: ConnectRpcService, rpc_path: String) {
    let (result, headers) = handle(service, &rpc_path, &request.message).await;
    if let Err(e) = &result {
        let span = Span::current();
        span.record("otel.status_code", "ERROR");
        span.record("otel.status_message", e.status.as_str());
        span.record("messaging.nats.service_error_code", e.code as u64);
    }
    if let Err(e) = request.respond_with_headers(result, headers).await {
        tracing::error!(rpc = %rpc_path, error = %e, "nats: failed to reply");
    }
}

/// Consumer span for one incoming request, continuing the caller's trace from the
/// NATS headers. The connect interceptor's `rpc.server` span becomes its child.
fn receive_span(msg: &async_nats::Message) -> Span {
    let subject = msg.subject.as_str();
    let span = tracing::info_span!(
        "nats.receive",
        otel.name = format!("process {subject}").as_str(),
        otel.kind = "consumer",
        otel.status_code = Empty,
        otel.status_message = Empty,
        messaging.system = "nats",
        messaging.operation.type = "process",
        messaging.destination.name = subject,
        messaging.message.body.size = msg.payload.len() as u64,
        messaging.nats.service_error_code = Empty,
    );
    continue_trace(&span, extract_nats(msg.headers.as_ref()));
    span
}

fn error_reply(status: u16, code: &str, message: String, mut headers: HeaderMap) -> Reply {
    headers.insert(CONNECT_CODE, code);
    let error = async_nats::service::error::Error { status: message, code: status as usize };
    (Err(error), headers)
}

/// Bridge one NATS request into the connect router and back.
async fn handle(service: ConnectRpcService, rpc_path: &str, msg: &async_nats::Message) -> Reply {
    let request = match to_http_request(rpc_path, msg) {
        Ok(r) => r,
        Err(e) => {
            let message = format!("bad request: {e}");
            return error_reply(400, "invalid_argument", message, HeaderMap::new());
        }
    };
    let Ok(response) = service.oneshot(request).await; // error type is Infallible
    let (parts, body) = response.into_parts();
    match body.collect().await {
        Ok(body) => to_reply(&parts, body.to_bytes()),
        Err(e) => error_reply(500, "internal", e.to_string(), HeaderMap::new()),
    }
}

/// NATS message → HTTP request the connect router understands.
fn to_http_request(rpc_path: &str, msg: &async_nats::Message) -> Result<Request<Full<Bytes>>, http::Error> {
    let mut request = Request::post(rpc_path)
        .header("connect-protocol-version", "1")
        .extension(Transport::Nats { subject: msg.subject.to_string() })
        .body(Full::new(msg.payload.clone()))?;
    let headers = request.headers_mut();
    for (name, values) in msg.headers.iter().flat_map(|h| h.iter()) {
        let Ok(name) = HeaderName::from_bytes(AsRef::<str>::as_ref(name).as_bytes()) else {
            continue;
        };
        if is_propagation_header(name.as_str()) {
            continue; // replaced by the nats.receive span's context below
        }
        for value in values.iter().filter_map(|v| HeaderValue::from_str(v.as_str()).ok()) {
            headers.append(name.clone(), value);
        }
    }
    if !headers.contains_key(http::header::CONTENT_TYPE) {
        headers.insert(http::header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    inject_http(&Span::current().context(), headers);
    Ok(request)
}

/// HTTP response from the connect router → NATS reply.
fn to_reply(parts: &response::Parts, body: Bytes) -> Reply {
    let mut headers = HeaderMap::new();
    for (name, value) in parts.headers.iter() {
        if let Ok(v) = value.to_str() {
            headers.append(name.as_str(), v);
        }
    }
    if parts.status == StatusCode::OK {
        return (Ok(body), headers);
    }
    let (code, message) = parse_connect_error(&body);
    error_reply(parts.status.as_u16(), &code, message, headers)
}
