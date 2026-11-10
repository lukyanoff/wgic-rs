//! The kernel is everything a service is allowed to know about the outside world.
//!
//! - [`Module`]: how a service plugs into the host.
//! - [`Deps`]: what a service gets from the host (a [`Channel`] to call other services, config).
//! - [`Channel`]: one client transport type for HTTP, NATS or in-process calls.
//! - [`Logger`], [`Metrics`]: injected logging and metrics (`deps.logger("greet")`, `deps.metrics("greet")`).
//! - [`kv`]: a structured field for logs and metric tags.
//! - [`RequestContextExt`]: read how a request arrived (`ctx.transport()`).
//! - [`testing`]: in-memory logs/metrics for tests.

mod channel;
mod deps;
mod field;
mod logger;
mod metrics;
mod module;
pub mod testing;
mod transport;

pub use channel::{Channel, ChannelBody};
pub use deps::Deps;
pub use field::{Field, FieldValue, kv};
pub use logger::{Level, LogBackend, Logger};
pub use metrics::{MetricKind, Metrics, MetricsBackend};
pub use module::Module;
pub use transport::{RequestContextExt, Transport};
