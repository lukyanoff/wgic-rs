//! Box 2: infrastructure. NATS micro serving (`server`), calling (`client`), the reply-inbox wire format (`wire`),
//! CAS instance state (`state`) and OpenTelemetry (`telemetry`). Knows no specific service.

pub mod client;
pub mod server;
pub mod state;
pub mod telemetry;
pub mod wire;

// Re-export pure abstractions from `contracts` so `sdk` users have a unified API
pub use client::{NatsLlmCallerClient, NatsLlmRegistry, NatsRpcClient, NatsSensorClient, NatsSensorRegistry};
pub use contracts::{ClientRegistry, MapRegistry, ProgressEvent, ProgressSender, RawRpcEndpoint, RpcError};
pub use server::{MAX_CALL, serve_endpoint};
