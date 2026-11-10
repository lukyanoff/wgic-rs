//! Client factory for `visioner` instances.

use std::time::Duration;

use contracts::proto::visioner::v1::VisionerServiceClient;
use kernel::{Channel, Deps};

/// Makes clients for `visioner` instances (≈ a C# typed client factory):
///
/// ```ignore
/// let client = VisionerClients::new(&deps).get("vision-1", Duration::from_secs(10));
/// ```
#[derive(Clone)]
pub struct VisionerClients {
    deps: Deps,
}

impl VisionerClients {
    pub fn new(deps: &Deps) -> Self {
        Self { deps: deps.clone() }
    }

    /// A client for the instance `instance` (over NATS: the host with that `NATS_PREFIX`).
    /// Every call gets `timeout` as its deadline; the server stops the call when it passes.
    pub fn get(&self, instance: &str, timeout: Duration) -> VisionerServiceClient<Channel> {
        let config = self.deps.client_config().with_default_timeout(timeout);
        VisionerServiceClient::new(self.deps.channel_to(instance), config)
    }
}
