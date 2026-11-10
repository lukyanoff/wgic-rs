use std::collections::HashMap;
use std::sync::Arc;

use connectrpc::Router;
use connectrpc::client::ClientConfig;

use crate::channel::SameChannel;
use crate::testing::{NoopLogs, NoopMetrics};
use crate::{Channel, ChannelFactory, LogBackend, Logger, Metrics, MetricsBackend, Module};

/// What the host gives a service when it is created (≈ a C# DI container, but explicit).
///
/// Logging and metrics: `deps.logger("greet")`, `deps.metrics("greet")`.
///
/// Call another service with its generated client:
/// `BillingServiceClient::new(deps.channel(), deps.client_config())`.
/// Whether that call goes over HTTP, NATS or stays in-process is the host's decision.
///
/// Call one named instance (e.g. `screen-1` of several screen sensors) through `channel_to("screen-1")`,
/// or better through its client factory in `libs/clients`.
#[derive(Clone)]
pub struct Deps {
    channel: Channel,
    channels: Arc<dyn ChannelFactory>,
    client_config: ClientConfig,
    logs: Arc<dyn LogBackend>,
    metrics: Arc<dyn MetricsBackend>,
    /// Test values that win over the process environment (see [`Deps::with_env`]).
    env: Arc<HashMap<String, String>>,
}

impl Deps {
    /// Logs and metrics are dropped until [`Deps::with_logs`] / [`Deps::with_metrics`] set them.
    pub fn new(channel: Channel, client_config: ClientConfig) -> Self {
        let channels = Arc::new(SameChannel(channel.clone()));
        let (logs, metrics) = (Arc::new(NoopLogs), Arc::new(NoopMetrics));
        Self { channel, channels, client_config, logs, metrics, env: Arc::default() }
    }

    /// Use `factory` for [`Deps::channel_to`] (platform's NATS factory, or a test one).
    pub fn with_channels(mut self, factory: impl ChannelFactory + 'static) -> Self {
        self.channels = Arc::new(factory);
        self
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

    /// Settings for tests, instead of the process environment: `.with_env([("SENSOR_KIND", "camera")])`.
    pub fn with_env<'a>(mut self, values: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut env = (*self.env).clone();
        for (key, value) in values {
            env.insert(key.to_string(), value.to_string());
        }
        self.env = Arc::new(env);
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
        let router = router_for(modules);
        let config = ClientConfig::new("http://in-process".parse().expect("valid uri"));
        Self::new(Channel::in_process(router), config)
    }

    /// Deps where each instance id has its own in-process services. For tests of a service that calls
    /// several instances of the same service:
    ///
    /// ```ignore
    /// let deps = Deps::in_process_instances(vec![("llm-a", vec![llm_a]), ("llm-b", vec![llm_b])]);
    /// let client = LlmCallerServiceClient::new(deps.channel_to("llm-b"), deps.client_config());
    /// ```
    /// An unknown id gets an empty router (calls answer `unimplemented`).
    pub fn in_process_instances(instances: Vec<(&str, InstanceModules)>) -> Self {
        let mut channels = HashMap::new();
        for (id, modules) in instances {
            channels.insert(id.to_string(), Channel::in_process(router_for(&modules)));
        }
        let factory = InstanceChannels { channels, unknown: Channel::in_process(Router::new()) };
        let config = ClientConfig::new("http://in-process".parse().expect("valid uri"));
        Self::new(factory.unknown.clone(), config).with_channels(factory)
    }

    /// Transport for calls to other services (on this host's own NATS prefix).
    pub fn channel(&self) -> Channel {
        self.channel.clone()
    }

    /// Transport for calls to the service instance `instance` (over NATS: the host with that `NATS_PREFIX`).
    pub fn channel_to(&self, instance: &str) -> Channel {
        self.channels.channel_to(instance)
    }

    /// Client settings (base URI, codec) that match [`Deps::channel`].
    pub fn client_config(&self) -> ClientConfig {
        self.client_config.clone()
    }

    /// A configuration value (from the environment), e.g. `deps.env("GREETING_PREFIX")`. Blank = not set.
    pub fn env(&self, key: &str) -> Option<String> {
        let value = match self.env.get(key) {
            Some(value) => Some(value.clone()),
            None => std::env::var(key).ok(),
        };
        value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    }

    /// The setting `key` read by `parse`, or `default` when it is not set (≈ binding an options class).
    /// Call it while building a module: a bad value stops the host with `KEY="x" is invalid, expected ...`.
    ///
    /// ```ignore
    /// let warmup = deps.setting("CAMERA_WARMUP", 5, "0..100", |v| v.parse().ok().filter(|n| *n <= 100));
    /// ```
    pub fn setting<T>(&self, key: &str, default: T, expected: &str, parse: impl Fn(&str) -> Option<T>) -> T {
        let Some(value) = self.env(key) else {
            return default;
        };
        match parse(&value) {
            Some(parsed) => parsed,
            None => panic!("{key}={value:?} is invalid, expected {expected}"),
        }
    }

    /// The setting `key`, which must be set: a missing one stops the host with `KEY is not set, expected ...`.
    pub fn required(&self, key: &str, expected: &str) -> String {
        match self.env(key) {
            Some(value) => value,
            None => panic!("{key} is not set, expected {expected}"),
        }
    }
}

/// The services one test instance runs (see [`Deps::in_process_instances`]).
pub type InstanceModules = Vec<Arc<dyn Module>>;

fn router_for(modules: &[Arc<dyn Module>]) -> Router {
    modules.iter().fold(Router::new(), |router, module| module.register(router))
}

/// [`Deps::in_process_instances`]: one in-process channel per instance id.
struct InstanceChannels {
    channels: HashMap<String, Channel>,
    unknown: Channel,
}

impl ChannelFactory for InstanceChannels {
    fn channel_to(&self, instance: &str) -> Channel {
        self.channels.get(instance).unwrap_or(&self.unknown).clone()
    }
}

#[cfg(test)]
#[path = "deps_tests.rs"]
mod tests;
