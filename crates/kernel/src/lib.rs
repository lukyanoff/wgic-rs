//! The kernel is everything a service is allowed to know about the outside world.
//!
//! - [`Module`]: how a service plugs into the host.
//! - [`Deps`]: what a service gets from the host (a [`Channel`] to call other services, config).
//! - [`Channel`]: one client transport type for HTTP, NATS or in-process calls.
//! - [`RequestContextExt`]: read how a request arrived (`ctx.transport()`).

mod channel;
mod deps;
mod module;
mod transport;

pub use channel::{Channel, ChannelBody};
pub use deps::Deps;
pub use module::Module;
pub use transport::{RequestContextExt, Transport};
