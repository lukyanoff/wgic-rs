//! Client factory for `greet` instances.

use std::time::Duration;

use contracts::proto::greet::v1::GreetServiceClient;
use kernel::{Channel, Deps};

/// Makes clients for `greet` instances (≈ a C# typed client factory):
///
/// ```ignore
/// let client = GreetClients::new(&deps).get("rpc", Duration::from_secs(10));
/// ```
#[derive(Clone)]
pub struct GreetClients {
    deps: Deps,
}

impl GreetClients {
    pub fn new(deps: &Deps) -> Self {
        Self { deps: deps.clone() }
    }

    /// A client for the instance `instance` (over NATS: the host with that `NATS_PREFIX`).
    /// Every call gets `timeout` as its deadline; the server stops the call when it passes.
    pub fn get(&self, instance: &str, timeout: Duration) -> GreetServiceClient<Channel> {
        let config = self.deps.client_config().with_default_timeout(timeout);
        GreetServiceClient::new(self.deps.channel_to(instance), config)
    }
}
