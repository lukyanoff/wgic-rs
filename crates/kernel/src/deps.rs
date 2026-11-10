use std::sync::Arc;

use connectrpc::Router;
use connectrpc::client::ClientConfig;

use crate::testing::{NoopLogs, NoopMetrics};
use crate::{Channel, LogBackend, Logger, Metrics, MetricsBackend, Module};

/// What the host gives a service when it is created (≈ a C# DI container, but explicit).
///
/// Logging and metrics: `deps.logger("greet")`, `deps.metrics("greet")`.
///
/// Call another service with its generated client:
/// `BillingServiceClient::new(deps.channel(), deps.client_config())`.
/// Whether that call goes over HTTP, NATS or stays in-process is the host's decision.
#[derive(Clone)]
pub struct Deps {
    channel: Channel,
    client_config: ClientConfig,
    logs: Arc<dyn LogBackend>,
    metrics: Arc<dyn MetricsBackend>,
}

impl Deps {
    /// Logs and metrics are dropped until [`Deps::with_logs`] / [`Deps::with_metrics`] set them.
    pub fn new(channel: Channel, client_config: ClientConfig) -> Self {
        Self { channel, client_config, logs: Arc::new(NoopLogs), metrics: Arc::new(NoopMetrics) }
    }

    /// Send logs to `backend` (platform's `TracingLogs`, or `testing::MemoryLogs`).
    pub fn with_logs(mut self, backend: impl LogBackend + 'static) -> Self {
        self.logs = Arc::new(backend);
        self
    }

    /// Send metrics to `backend` (platform's `OtelMetrics`, or `testing::MemoryMetrics`).
    pub fn with_metrics(mut self, backend: impl MetricsBackend + 'static) -> Self {
        self.metrics = Arc::new(backend);
        self
    }

    /// A logger for `category` (usually the service name).
    pub fn logger(&self, category: &str) -> Logger {
        Logger::new(category, self.logs.clone())
    }

    /// Metrics named `<meter>.<name>` (usually meter = the service name).
    pub fn metrics(&self, meter: &str) -> Metrics {
        Metrics::new(meter, self.metrics.clone())
    }

    /// Deps whose channel calls `modules` in-process. For tests:
    ///
    /// ```ignore
    /// let greet = greet::module(&Deps::in_process(&[]));   // greet calls nobody
    /// let deps = Deps::in_process(&[greet]);                // the view of a caller
    /// let client = GreetServiceClient::new(deps.channel(), deps.client_config());
    /// ```
    pub fn in_process(modules: &[Arc<dyn Module>]) -> Self {
        let router = modules.iter().fold(Router::new(), |router, module| module.register(router));
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
