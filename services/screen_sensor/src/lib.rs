//! Screen sensor service: takes a picture of a screen and returns the image + metadata.
//!
//! Built on `sensor_base` (the shared "base class"); this crate only adds screen drivers.
//! - `config.rs`  — screen-only setting: `SCREEN_SENSOR_DRIVER`
//! - `drivers/`   — one file per capture mechanism (`xcap`, `linux-x11`) + the `create` factory
//! - `handler.rs` — implements the generated `ScreenSensorService` by delegating to `SensorRpc`
//! - `module()`   — the only thing the host needs

// ---------------------------------------------------------------------------------------------
// Modules and usings
// ---------------------------------------------------------------------------------------------

mod config;
mod drivers;
mod handler;

use std::sync::Arc;

use connectrpc::Router;
use kernel::{Deps, Module};
use sensor_base::{FrameSource, Sensor, SensorConfig, SensorRpc, SensorSettings};

use handler::ScreenSensorHandler;

// ---------------------------------------------------------------------------------------------
// Composition root
// ---------------------------------------------------------------------------------------------

/// Create the module from env: `SCREEN_SENSOR_ID`, `_DEVICE`, `_FORMAT`, `_DRIVER`.
/// A bad setting stops the host at startup with a clear message, rather than failing every call.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    let settings = match SensorSettings::from_env(deps, config::ENV_PREFIX, config::DEFAULT_ID) {
        Ok(settings) => settings,
        Err(error) => panic!("screen_sensor: {error}"),
    };
    let driver = match config::driver_from_env(deps) {
        Ok(driver) => driver,
        Err(error) => panic!("screen_sensor: {error}"),
    };
    module_with(deps, Arc::new(settings), drivers::create(driver))
}

/// Create the module with an explicit config and driver (≈ registering your own
/// `IFrameSource` in a C# DI container). Tests use it to inject `sensor_base::testing::FakeSource`.
pub fn module_with(deps: &Deps, config: Arc<dyn SensorConfig>, source: Arc<dyn FrameSource>) -> Arc<dyn Module> {
    let rpc = SensorRpc::new(deps, "screen_sensor", Sensor::new(config, source));
    Arc::new(ScreenSensorModule { handler: Arc::new(ScreenSensorHandler::new(rpc)) })
}

struct ScreenSensorModule {
    handler: Arc<ScreenSensorHandler>,
}

impl Module for ScreenSensorModule {
    fn name(&self) -> &'static str {
        "screen_sensor"
    }

    fn register(&self, router: Router) -> Router {
        router.add_service(self.handler.clone())
    }
}
