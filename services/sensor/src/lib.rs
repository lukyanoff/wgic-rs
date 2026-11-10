//! Sensor service: takes a picture of a screen or from a camera and returns the image + metadata.
//!
//! - `config.rs`  — `SENSOR_*` settings
//! - `drivers.rs` — `FrameSource`, the interface each capture mechanism implements (`Screen`, `Camera`)
//! - `handler.rs` — the RPCs: grab a frame, encode it, describe it

mod config;
pub mod drivers;
mod handler;

use std::sync::Arc;

use kernel::{Deps, Module};

pub use config::{ImageFormat, SensorSettings};
use drivers::FrameSource;
use handler::SensorHandler;

/// Called by the host once at startup. `SENSOR_KIND` picks the driver; a bad setting stops the host.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    let source = deps.setting("SENSOR_KIND", drivers::screen(), "screen or camera", drivers::by_kind);
    module_with(SensorSettings::from_env(deps, source.kind()), source)
}

/// The module with explicit settings and driver (tests pass `drivers::FakeSource`).
pub fn module_with(settings: SensorSettings, source: Arc<dyn FrameSource>) -> Arc<dyn Module> {
    kernel::module("sensor", SensorHandler { settings, source })
}
