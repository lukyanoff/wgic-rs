//! Client factory for `llm_caller` instances.

use std::time::Duration;

use contracts::proto::llm_caller::v1::LlmCallerServiceClient;
use kernel::{Channel, Deps};

/// Makes clients for `llm_caller` instances (≈ a C# typed client factory):
///
/// ```ignore
/// let client = LlmCallerClients::new(&deps).get("llm-claude", Duration::from_secs(10));
/// ```
#[derive(Clone)]
pub struct LlmCallerClients {
    deps: Deps,
}

impl LlmCallerClients {
    pub fn new(deps: &Deps) -> Self {
        Self { deps: deps.clone() }
    }

    /// A client for the instance `instance` (over NATS: the host with that `NATS_PREFIX`).
    /// Every call gets `timeout` as its deadline; the server stops the call when it passes.
    pub fn get(&self, instance: &str, timeout: Duration) -> LlmCallerServiceClient<Channel> {
        let config = self.deps.client_config().with_default_timeout(timeout);
        LlmCallerServiceClient::new(self.deps.channel_to(instance), config)
    }
}
