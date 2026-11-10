use std::net::SocketAddr;
use std::time::Duration;

use axum::http::{HeaderName, HeaderValue, Method};
use connectrpc::ConnectRpcService;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub mod nats;
pub use nats::{NatsConfig, NatsTransport, serve_nats};

pub mod proto {
    pub mod greet {
        pub mod v1 {
            include!("gen/buffa/greet.v1.rs");
            include!("gen/connect/greet.v1.rs");
        }
    }
}

/// Address to listen on / connect to, from env `ADDR` (default `127.0.0.1:8080`).
pub fn addr_from_env() -> String {
    std::env::var("ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into())
}

/// CORS for browser clients (Connect, gRPC-web).
///
/// Allowed origins come from env `CORS_ORIGINS` (comma-separated,
/// default `http://localhost:5173`); `*` allows any origin.
pub fn cors() -> CorsLayer {
    let origins = std::env::var("CORS_ORIGINS").unwrap_or_else(|_| "http://localhost:5173".into());
    let allow_origin = if origins.trim() == "*" {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(
            origins
                .split(',')
                .filter_map(|o| HeaderValue::from_str(o.trim()).ok()),
        )
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

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Serve a connectrpc router with CORS enabled (HTTP/1.1 + HTTP/2 cleartext).
pub async fn serve(router: connectrpc::Router, addr: SocketAddr) -> Result<(), BoxError> {
    serve_http(ConnectRpcService::new(router), addr).await
}

async fn serve_http(service: ConnectRpcService, addr: SocketAddr) -> Result<(), BoxError> {
    let app = axum::Router::new().fallback_service(service).layer(cors());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("http: listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Run a router on every enabled adapter:
/// - HTTP (Connect, gRPC, gRPC-web) on `ADDR`, unless `HTTP_DISABLED=1`
/// - NATS micro if `NATS_URL` is set (see [`NatsConfig::from_env`])
pub async fn run(router: connectrpc::Router) -> Result<(), BoxError> {
    let methods: Vec<String> = router.methods().map(str::to_string).collect();
    let service = ConnectRpcService::new(router);
    let http_enabled = !matches!(
        std::env::var("HTTP_DISABLED").as_deref(),
        Ok("1") | Ok("true")
    );
    let nats_cfg = NatsConfig::from_env();
    if !http_enabled && nats_cfg.is_none() {
        return Err("nothing to serve: HTTP_DISABLED is set and NATS_URL is not".into());
    }

    let http = async {
        if http_enabled {
            serve_http(service.clone(), addr_from_env().parse()?).await
        } else {
            Ok(())
        }
    };
    let nats = async {
        match nats_cfg {
            Some(cfg) => {
                let nc = async_nats::ConnectOptions::new()
                    .name(format!("{}-server", cfg.name))
                    .connect(cfg.url.as_str())
                    .await?;
                serve_nats(service.clone(), methods, nc, cfg).await
            }
            None => Ok(()),
        }
    };
    tokio::try_join!(http, nats)?;
    Ok(())
}
