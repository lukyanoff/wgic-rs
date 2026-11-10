//! Shared base for image sensor services (`screen_sensor`, `camera_sensor`, ...).
//!
//! - `config.rs`  — `SensorConfig` interface (id, device, format) + `SensorSettings` (env-backed) + parsers
//! - `domain.rs`  — `FrameSource` (the abstract part each driver implements) + `Sensor` (the "base class")
//! - `rpc.rs`     — `SensorRpc`: the shared handler logic a service delegates to
//! - `testing.rs` — `FakeSource` for services' tests
//! - `unsupported.rs` — stand-in driver for "this driver doesn't exist on this OS"

// ---------------------------------------------------------------------------------------------
// Modules (≈ C# namespaces/files)
// ---------------------------------------------------------------------------------------------

mod config;
mod domain;
mod rpc;
pub mod testing;
mod unsupported;

// ---------------------------------------------------------------------------------------------
// Public API: `pub use` re-exports, so services write `sensor_base::Sensor`
// ---------------------------------------------------------------------------------------------

pub use config::{
    ConfigError, DeviceSelector, ImageFormat, SensorConfig, SensorSettings, invalid, parse_device, parse_format,
};
pub use domain::{
    Capture, CaptureError, CaptureMetadata, DeviceInfo, FrameSource, RawFrame, Sensor, pick_device, rgb_to_rgba,
};
pub use rpc::{CaptureReply, SensorRpc};
pub use unsupported::UnsupportedSource;
