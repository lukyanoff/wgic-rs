//! Box 2: infrastructure. Exposes services over HTTP and NATS, wires telemetry,
//! and runs a set of [`kernel::Module`]s. Knows nothing about any domain or proto.
//!
//! Entry points for `apps/*`:
//! - [`init_telemetry`] — first line of every `main`.
//! - [`run`] — serve modules on every adapter enabled by the environment.
//! - [`select_modules`] — pick modules by `SERVICES=greet,billing`.
//! - [`deps_from_env`] — a [`kernel::Deps`] whose channel calls other services over HTTP or NATS.

mod adapter;
mod host;
mod http_server;
pub mod nats;
pub mod telemetry;

pub use adapter::{Adapter, HttpAdapter, NatsAdapter};
pub use host::{Modules, deps_from_env, run, select_modules};
pub use http_server::cors;
pub use telemetry::{TelemetryGuard, init as init_telemetry};

use connectrpc::{ConnectRpcService, Router};

/// The connect service with the OTEL interceptor, shared by every adapter.
pub fn traced_service(router: Router) -> ConnectRpcService {
    ConnectRpcService::new(router).with_interceptor(telemetry::OtelInterceptor)
}

/// Address to listen on / connect to, from env `ADDR` (default `127.0.0.1:8080`).
pub fn addr_from_env() -> String {
    std::env::var("ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into())
}
