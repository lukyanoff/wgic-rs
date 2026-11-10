//! Screen sensor service: takes a picture of a screen and returns the image + metadata.
//!
//! - `config.rs`  — `ScreenSensorConfig` interface (screen, format, driver) + `SensorConfig` (env-backed)
//! - `domain.rs`  — `CaptureDriver` interface + `ScreenSensor` class (capture, encode, metadata)
//! - `drivers/`   — one file per capture mechanism (`xcap`, `linux-x11`, ...) + the `create` factory
//! - `handler.rs` — proto ⇄ domain + logs/metrics (implements the generated `ScreenSensorService` trait)
//! - `module()`   — the only thing the host needs

// ---------------------------------------------------------------------------------------------
// Modules (≈ C# namespaces/files) and public API
// ---------------------------------------------------------------------------------------------

mod config;
mod domain;
mod drivers;
mod handler;

// `pub use` re-exports these so tests and other crates can write `screen_sensor::SensorConfig`.
pub use config::{ConfigError, DriverKind, ImageFormat, ScreenSelector, ScreenSensorConfig, SensorConfig};
pub use domain::{CaptureDriver, CaptureError, RawFrame, ScreenInfo, pick_screen};

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use connectrpc::Router;
use kernel::{Deps, Module};

use domain::ScreenSensor;
use handler::ScreenSensorHandler;

// ---------------------------------------------------------------------------------------------
// Composition root
// ---------------------------------------------------------------------------------------------

/// Create the module from environment config (`SCREEN_SENSOR_SCREEN/FORMAT/DRIVER`).
/// A bad setting stops the host at startup with a clear message, rather than failing every call.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    let config = match SensorConfig::from_env(deps) {
        Ok(config) => config,
        Err(error) => panic!("screen_sensor: {error}"),
    };
    let driver = drivers::create(config.driver());
    module_with(deps, Arc::new(config), driver)
}

/// Create the module with an explicit config and driver (≈ registering your own
/// `ICaptureDriver` in a C# DI container). Tests use it to inject a fake driver.
pub fn module_with(
    deps: &Deps,
    config: Arc<dyn ScreenSensorConfig>,
    driver: Arc<dyn CaptureDriver>,
) -> Arc<dyn Module> {
    let sensor = ScreenSensor::new(config, driver);
    let handler = ScreenSensorHandler::new(deps, Arc::new(sensor));
    Arc::new(ScreenSensorModule { handler: Arc::new(handler) })
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
