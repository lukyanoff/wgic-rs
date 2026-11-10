//! NATS micro adapter for connectrpc.
//!
//! NATS carries Connect-protocol **unary** messages, so every service registered in a
//! [`connectrpc::Router`] can be hosted on NATS ([`serve_nats`]) and every generated
//! client can call over NATS ([`NatsTransport`]), with no per-service code.
//!
//! Wire format:
//! - subject: `<prefix>.<package.Service>.<Method>`, e.g. `rpc.greet.v1.GreetService.Greet`
//! - micro endpoint name: `<Service>-<Method>`, e.g. `GreetService-Greet`
//! - request: payload = message, header `Content-Type` = `application/json` (default) or
//!   `application/proto`; other headers are passed through as RPC metadata
//! - success reply: payload = message, header `Content-Type`
//! - error reply: empty payload, headers `Nats-Service-Error` (message),
//!   `Nats-Service-Error-Code` (HTTP status) and `Connect-Code` (e.g. `invalid_argument`)

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use async_nats::service::ServiceExt;
use async_nats::{HeaderMap, HeaderName as NatsHeaderName, HeaderValue as NatsHeaderValue};
use bytes::Bytes;
use connectrpc::client::{ClientBody, ClientTransport};
use connectrpc::{ConnectError, ConnectRpcService, ErrorCode};
use futures::StreamExt;
use futures::future::BoxFuture;
use http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use tower::ServiceExt as _;

pub const CONNECT_CODE: &str = "Connect-Code";
const NATS_SERVICE_ERROR: &str = "Nats-Service-Error";
const NATS_SERVICE_ERROR_CODE: &str = "Nats-Service-Error-Code";

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Settings for the NATS adapter, usually from the environment.
#[derive(Debug, Clone)]
pub struct NatsConfig {
    /// NATS server URL(s), comma-separated.
    pub url: String,
    /// Subject prefix for all RPC endpoints.
    pub prefix: String,
    /// Micro service name (`[A-Za-z0-9_-]`), shared by all instances of this server.
    pub name: String,
    /// Micro service version (semver).
    pub version: String,
    pub description: String,
}

impl NatsConfig {
    /// Reads `NATS_URL` (required), `NATS_PREFIX` (default `rpc`),
    /// `NATS_SERVICE_NAME` (default `greet`) and `NATS_SERVICE_VERSION`
    /// (default: crate version). Returns `None` if `NATS_URL` is not set.
    pub fn from_env() -> Option<Self> {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        Some(Self {
            url: std::env::var("NATS_URL").ok()?,
            prefix: env("NATS_PREFIX", "rpc"),
            name: env("NATS_SERVICE_NAME", "greet"),
            version: env("NATS_SERVICE_VERSION", env!("CARGO_PKG_VERSION")),
            description: env(
                "NATS_SERVICE_DESCRIPTION",
                "connectrpc services over NATS micro",
            ),
        })
    }
}

/// `greet.v1.GreetService/Greet` → `rpc.greet.v1.GreetService.Greet`
pub fn subject_for(prefix: &str, path: &str) -> String {
    let path = path.trim_start_matches('/').replace('/', ".");
    if prefix.is_empty() {
        path
    } else {
        format!("{prefix}.{path}")
    }
}

/// Short endpoint name `GreetService-Greet`, or the fully-qualified
/// `greet_v1_GreetService_Greet` when the short one is taken.
fn endpoint_name(path: &str, taken: &mut HashSet<String>) -> String {
    let path = path.trim_start_matches('/');
    let (service, method) = path.rsplit_once('/').unwrap_or(("", path));
    let short_service = service.rsplit('.').next().unwrap_or(service);
    let sanitize = |s: String| {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
    };
    let short = sanitize(format!("{short_service}-{method}"));
    let name = if taken.contains(&short) {
        sanitize(format!("{service}_{method}"))
    } else {
        short
    };
    taken.insert(name.clone());
    name
}

/// Host every method of `service` as a NATS micro endpoint. Runs until the
/// connection closes. All instances with the same `cfg.name` share a queue
/// group, so NATS load-balances requests across them.
pub async fn serve_nats(
    service: ConnectRpcService,
    methods: Vec<String>,
    nc: async_nats::Client,
    cfg: NatsConfig,
) -> Result<(), BoxError> {
    let svc = nc
        .service_builder()
        .description(&cfg.description)
        .start(&cfg.name, &cfg.version)
        .await?;

    let mut taken = HashSet::new();
    let mut tasks = Vec::new();
    for path in methods {
        let subject = subject_for(&cfg.prefix, &path);
        let name = endpoint_name(&path, &mut taken);
        let rpc_path = format!("/{}", path.trim_start_matches('/'));
        let mut endpoint = svc
            .endpoint_builder()
            .name(&name)
            .metadata(HashMap::from([("rpc".to_string(), rpc_path.clone())]))
            .add(&subject)
            .await?;
        println!("nats: {name} on {subject}");

        let service = service.clone();
        tasks.push(tokio::spawn(async move {
            while let Some(req) = endpoint.next().await {
                let service = service.clone();
                let rpc_path = rpc_path.clone();
                tokio::spawn(async move {
                    let (result, headers) = handle(service, &rpc_path, &req.message).await;
                    if let Err(e) = req.respond_with_headers(result, headers).await {
                        eprintln!("nats: failed to reply on {rpc_path}: {e}");
                    }
                });
            }
        }));
    }
    println!(
        "nats micro service {} {} started ({})",
        cfg.name, cfg.version, cfg.url
    );
    for t in tasks {
        t.await?;
    }
    Ok(())
}

/// Bridge one NATS request into the connect router and back.
async fn handle(
    service: ConnectRpcService,
    rpc_path: &str,
    msg: &async_nats::Message,
) -> (Result<Bytes, async_nats::service::error::Error>, HeaderMap) {
    let mut reply_headers = HeaderMap::new();
    let error = |status: u16, code: &str, message: String, mut h: HeaderMap| {
        h.insert(CONNECT_CODE, code);
        (
            Err(async_nats::service::error::Error {
                status: message,
                code: status as usize,
            }),
            h,
        )
    };

    let mut builder = Request::post(rpc_path).header("connect-protocol-version", "1");
    let mut has_content_type = false;
    if let Some(h) = &msg.headers {
        for (name, values) in h.iter() {
            let Ok(name) = HeaderName::from_bytes(AsRef::<str>::as_ref(name).as_bytes()) else {
                continue;
            };
            has_content_type |= name == http::header::CONTENT_TYPE;
            for v in values {
                if let Ok(v) = HeaderValue::from_str(v.as_str()) {
                    builder = builder.header(name.clone(), v);
                }
            }
        }
    }
    if !has_content_type {
        builder = builder.header(http::header::CONTENT_TYPE, "application/json");
    }
    let request = match builder.body(Full::new(msg.payload.clone())) {
        Ok(r) => r,
        Err(e) => {
            return error(
                400,
                "invalid_argument",
                format!("bad request: {e}"),
                reply_headers,
            );
        }
    };

    let response = match service.oneshot(request).await {
        Ok(r) => r,
        Err(e) => return error(500, "internal", e.to_string(), reply_headers),
    };
    let (parts, body) = response.into_parts();
    let body = match body.collect().await {
        Ok(b) => b.to_bytes(),
        Err(e) => return error(500, "internal", e.to_string(), reply_headers),
    };
    for (name, value) in parts.headers.iter() {
        if let Ok(v) = value.to_str() {
            reply_headers.append(name.as_str(), v);
        }
    }

    if parts.status == StatusCode::OK {
        return (Ok(body), reply_headers);
    }
    // Connect unary errors are JSON: {"code": "...", "message": "..."}
    let (code, message) = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .map(|v| {
            (
                v["code"].as_str().unwrap_or("unknown").to_string(),
                v["message"].as_str().unwrap_or_default().to_string(),
            )
        })
        .unwrap_or_else(|| ("unknown".into(), String::from_utf8_lossy(&body).into()));
    let message = if message.is_empty() {
        code.clone()
    } else {
        message
    };
    error(parts.status.as_u16(), &code, message, reply_headers)
}

/// `ClientTransport` that sends Connect unary calls over NATS request/reply,
/// so any generated client works over NATS:
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
        Self {
            nc,
            prefix: prefix.into(),
        }
    }
}

impl ClientTransport for NatsTransport {
    type ResponseBody = Full<Bytes>;
    type Error = ConnectError;

    fn send(
        &self,
        request: Request<ClientBody>,
    ) -> BoxFuture<'static, Result<Response<Self::ResponseBody>, Self::Error>> {
        let nc = self.nc.clone();
        let prefix = self.prefix.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let subject = subject_for(&prefix, parts.uri.path());
            let payload = body.collect().await?.to_bytes();

            let mut headers = HeaderMap::new();
            for (name, value) in parts.headers.iter() {
                if let Ok(v) = value.to_str() {
                    headers.append(name.as_str(), v);
                }
            }
            let timeout = parts
                .headers
                .get("connect-timeout-ms")
                .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
                .map(Duration::from_millis);

            let req = async_nats::Request::new()
                .payload(payload)
                .headers(headers)
                .timeout(timeout);
            let reply = nc
                .send_request(subject.clone(), req)
                .await
                .map_err(|e| request_error(&subject, e))?;
            Ok(reply_to_response(reply))
        })
    }
}

fn request_error(subject: &str, e: async_nats::RequestError) -> ConnectError {
    use async_nats::RequestErrorKind::*;
    match e.kind() {
        NoResponders => ConnectError::new(
            ErrorCode::Unavailable,
            format!("no NATS responders for {subject}"),
        ),
        TimedOut => ConnectError::new(
            ErrorCode::DeadlineExceeded,
            format!("NATS request to {subject} timed out"),
        ),
        _ => ConnectError::new(ErrorCode::Unavailable, format!("NATS {subject}: {e}")),
    }
}

/// NATS reply → HTTP response the connect client understands.
fn reply_to_response(reply: async_nats::Message) -> Response<Full<Bytes>> {
    let headers = reply.headers.unwrap_or_default();
    let get = |k: &str| headers.get(k).map(|v| v.as_str().to_string());
    let mut builder = Response::builder();

    if let Some(status) = get(NATS_SERVICE_ERROR_CODE) {
        let status = status.parse::<u16>().unwrap_or(500);
        let code = get(CONNECT_CODE).unwrap_or_else(|| code_for_status(status).to_string());
        let message = get(NATS_SERVICE_ERROR).unwrap_or_default();
        let body = serde_json::json!({ "code": code, "message": message }).to_string();
        return builder
            .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body)))
            .expect("valid response");
    }

    for (name, values) in headers.iter() {
        let name: &NatsHeaderName = name;
        let Ok(name) = HeaderName::from_bytes(AsRef::<str>::as_ref(name).as_bytes()) else {
            continue;
        };
        for v in values {
            let v: &NatsHeaderValue = v;
            if let Ok(v) = HeaderValue::from_str(v.as_str()) {
                builder = builder.header(name.clone(), v);
            }
        }
    }
    builder
        .status(StatusCode::OK)
        .body(Full::new(reply.payload))
        .expect("valid response")
}

/// Connect's HTTP status → code mapping, for errors from non-connect micro services.
fn code_for_status(status: u16) -> &'static str {
    match status {
        400 => "invalid_argument",
        401 => "unauthenticated",
        403 => "permission_denied",
        404 => "unimplemented",
        408 | 504 => "deadline_exceeded",
        409 => "aborted",
        412 => "failed_precondition",
        413 | 429 => "resource_exhausted",
        499 => "canceled",
        501 => "unimplemented",
        502 | 503 => "unavailable",
        _ => "unknown",
    }
}
