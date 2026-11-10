use std::net::SocketAddr;
use std::time::Duration;

use axum::http::{HeaderName, HeaderValue, Method};
use connectrpc::ConnectRpcService;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// CORS for browser clients (Connect, gRPC-web).
///
/// Allowed origins come from env `CORS_ORIGINS` (comma-separated,
/// default `http://localhost:5173`); `*` allows any origin.
pub fn cors() -> CorsLayer {
    let origins = std::env::var("CORS_ORIGINS").unwrap_or_else(|_| "http://localhost:5173".into());
    let allow_origin = if origins.trim() == "*" {
        AllowOrigin::any()
    } else {
        let list = origins.split(',').filter_map(|o| HeaderValue::from_str(o.trim()).ok());
        AllowOrigin::list(list)
    };
    let h = HeaderName::from_static;
    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            h("content-type"),
            h("connect-protocol-version"),
            h("connect-timeout-ms"),
            h("connect-accept-encoding"),
            h("connect-content-encoding"),
            h("grpc-timeout"),
            h("x-grpc-web"),
            h("x-user-agent"),
            h("traceparent"),
            h("tracestate"),
            h("baggage"),
        ])
        .expose_headers([
            h("grpc-status"),
            h("grpc-message"),
            h("grpc-status-details-bin"),
            h("content-encoding"),
            h("connect-content-encoding"),
        ])
        .max_age(Duration::from_secs(7200))
}

/// Serve Connect, gRPC and gRPC-web (HTTP/1.1 + HTTP/2 cleartext) with CORS.
pub(crate) async fn serve_http(service: ConnectRpcService, addr: SocketAddr) -> anyhow::Result<()> {
    let app = axum::Router::new().fallback_service(service).layer(cors());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "http: listening");
    axum::serve(listener, app).await?;
    Ok(())
}
