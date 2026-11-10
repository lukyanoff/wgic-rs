//! Client factory for `camera_sensor` instances.

use std::time::Duration;

use contracts::proto::camera_sensor::v1::CameraSensorServiceClient;
use kernel::{Channel, Deps};

/// Makes clients for `camera_sensor` instances (≈ a C# typed client factory):
///
/// ```ignore
/// let client = CameraSensorClients::new(&deps).get("cam-1", Duration::from_secs(10));
/// ```
#[derive(Clone)]
pub struct CameraSensorClients {
    deps: Deps,
}

impl CameraSensorClients {
    pub fn new(deps: &Deps) -> Self {
        Self { deps: deps.clone() }
    }

    /// A client for the instance `instance` (over NATS: the host with that `NATS_PREFIX`).
    /// Every call gets `timeout` as its deadline; the server stops the call when it passes.
    pub fn get(&self, instance: &str, timeout: Duration) -> CameraSensorServiceClient<Channel> {
        let config = self.deps.client_config().with_default_timeout(timeout);
        CameraSensorServiceClient::new(self.deps.channel_to(instance), config)
    }
}
