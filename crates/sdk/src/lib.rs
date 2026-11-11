//! Box 2: infrastructure. NATS micro serving (`server`), calling (`client`), the reply-inbox wire format (`wire`),
//! CAS instance state (`state`), the injected service logger (`log`) and OpenTelemetry (`telemetry`). Knows no specific service.

pub mod client;
pub mod log;
pub mod server;
pub mod state;
pub mod telemetry;
pub mod wire;

// Re-export pure abstractions from `contracts` so `sdk` users have a unified API
pub use client::{NatsLlmCallerClient, NatsLlmRegistry, NatsRpcClient, NatsSensorClient, NatsSensorRegistry};
pub use contracts::{ClientRegistry, MapRegistry, ProgressEvent, ProgressSender, RawRpcEndpoint, RpcError};
pub use log::{ILogger, Level, LogEntry, MemoryLogger, OtelLogger, Value, json};
pub use server::{MAX_CALL, serve_endpoint};
