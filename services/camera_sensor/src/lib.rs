//! Camera sensor service: takes a picture from a webcam and returns the image + metadata.
//!
//! Built on `sensor_base` (the shared "base class"); this crate only adds camera drivers.
//! - `config.rs`  — camera-only settings: driver, resolution, warm-up frames
//! - `drivers/`   — nokhwa backends (V4L2 / AVFoundation / Media Foundation) + the `create` factory
//! - `handler.rs` — implements the generated `CameraSensorService` by delegating to `SensorRpc`
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

use config::CameraOptions;
use handler::CameraSensorHandler;

// ---------------------------------------------------------------------------------------------
// Composition root
// ---------------------------------------------------------------------------------------------

/// Create the module from env: `CAMERA_SENSOR_ID`, `_DEVICE`, `_FORMAT`, `_DRIVER`, `_RESOLUTION`,
/// `_WARMUP_FRAMES`. A bad setting stops the host at startup with a clear message.
pub fn module(deps: &Deps) -> Arc<dyn Module> {
    let settings = match SensorSettings::from_env(deps, config::ENV_PREFIX, config::DEFAULT_ID) {
        Ok(settings) => settings,
        Err(error) => panic!("camera_sensor: {error}"),
    };
    let options = match CameraOptions::from_env(deps) {
        Ok(options) => options,
        Err(error) => panic!("camera_sensor: {error}"),
    };
    request_camera_permission();
    module_with(deps, Arc::new(settings), drivers::create(options))
}

/// Create the module with an explicit config and driver (≈ registering your own
/// `IFrameSource` in a C# DI container). Tests use it to inject `sensor_base::testing::FakeSource`.
pub fn module_with(deps: &Deps, config: Arc<dyn SensorConfig>, source: Arc<dyn FrameSource>) -> Arc<dyn Module> {
    let rpc = SensorRpc::new(deps, "camera_sensor", Sensor::new(config, source));
    Arc::new(CameraSensorModule { handler: Arc::new(CameraSensorHandler::new(rpc)) })
}

/// macOS asks the user for camera access once; nokhwa wants that done before first use.
#[cfg(target_os = "macos")]
fn request_camera_permission() {
    nokhwa::nokhwa_initialize(|_granted| {});
}

/// Other OSes need no permission step.
#[cfg(not(target_os = "macos"))]
fn request_camera_permission() {}

struct CameraSensorModule {
    handler: Arc<CameraSensorHandler>,
}

impl Module for CameraSensorModule {
    fn name(&self) -> &'static str {
        "camera_sensor"
    }

    fn register(&self, router: Router) -> Router {
        router.add_service(self.handler.clone())
    }
}
