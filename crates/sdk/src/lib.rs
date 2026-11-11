pub mod state;
pub mod telemetry;
pub mod transport;
pub mod wire;

// Re-export pure abstractions from `contracts` so `sdk` users have a unified API
pub use contracts::{
    ClientRegistry, MapRegistry, ProgressSender, RawRpcEndpoint, RpcError,
};
pub use transport::{
    NatsLlmCallerClient, NatsLlmRegistry, NatsRpcClient, NatsSensorClient, NatsSensorRegistry,
    serve_endpoint,
};
