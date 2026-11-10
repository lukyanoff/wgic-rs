//! Client factory for `screen_sensor` instances.

use std::time::Duration;

use contracts::proto::screen_sensor::v1::ScreenSensorServiceClient;
use kernel::{Channel, Deps};

/// Makes clients for `screen_sensor` instances (≈ a C# typed client factory):
///
/// ```ignore
/// let client = ScreenSensorClients::new(&deps).get("screen-1", Duration::from_secs(10));
/// ```
#[derive(Clone)]
pub struct ScreenSensorClients {
    deps: Deps,
}

impl ScreenSensorClients {
    pub fn new(deps: &Deps) -> Self {
        Self { deps: deps.clone() }
    }

    /// A client for the instance `instance` (over NATS: the host with that `NATS_PREFIX`).
    /// Every call gets `timeout` as its deadline; the server stops the call when it passes.
    pub fn get(&self, instance: &str, timeout: Duration) -> ScreenSensorServiceClient<Channel> {
        let config = self.deps.client_config().with_default_timeout(timeout);
        ScreenSensorServiceClient::new(self.deps.channel_to(instance), config)
    }
}
