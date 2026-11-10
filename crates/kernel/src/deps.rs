use std::sync::Arc;

use connectrpc::Router;
use connectrpc::client::ClientConfig;

use crate::{Channel, Module};

/// What the host gives a service when it is created.
///
/// Call another service with its generated client:
/// `BillingServiceClient::new(deps.channel(), deps.client_config())`.
/// Whether that call goes over HTTP, NATS or stays in-process is the host's decision.
#[derive(Clone)]
pub struct Deps {
    channel: Channel,
    client_config: ClientConfig,
}

impl Deps {
    pub fn new(channel: Channel, client_config: ClientConfig) -> Self {
        Self {
            channel,
            client_config,
        }
    }

    /// Deps whose channel calls `modules` in-process. For tests:
    ///
    /// ```ignore
    /// let greet = greet::module(&Deps::in_process(&[]));   // greet calls nobody
    /// let deps = Deps::in_process(&[greet]);                // the view of a caller
    /// let client = GreetServiceClient::new(deps.channel(), deps.client_config());
    /// ```
    pub fn in_process(modules: &[Arc<dyn Module>]) -> Self {
        let router = modules
            .iter()
            .fold(Router::new(), |router, module| module.register(router));
        let config = ClientConfig::new("http://in-process".parse().expect("valid uri"));
        Self::new(Channel::in_process(router), config)
    }

    /// Transport for calls to other services.
    pub fn channel(&self) -> Channel {
        self.channel.clone()
    }

    /// Client settings (base URI, codec) that match [`Deps::channel`].
    pub fn client_config(&self) -> ClientConfig {
        self.client_config.clone()
    }

    /// A configuration value (from the environment), e.g. `deps.env("GREETING_PREFIX")`.
    pub fn env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.trim().is_empty())
    }
}
