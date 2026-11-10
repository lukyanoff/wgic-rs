//! Camera-only settings. Shared ones (`_ID`, `_DEVICE`, `_FORMAT`) come from `sensor_base::SensorSettings`.

// ---------------------------------------------------------------------------------------------
// Usings + constants
// ---------------------------------------------------------------------------------------------

use kernel::Deps;
use sensor_base::{ConfigError, invalid};

/// Env prefix for all camera settings: `CAMERA_SENSOR_ID`, `CAMERA_SENSOR_DEVICE`, ...
pub const ENV_PREFIX: &str = "CAMERA_SENSOR";
/// Instance id when `CAMERA_SENSOR_ID` is not set.
pub const DEFAULT_ID: &str = "camera";
pub const DRIVER_ENV: &str = "CAMERA_SENSOR_DRIVER";
pub const RESOLUTION_ENV: &str = "CAMERA_SENSOR_RESOLUTION";
pub const WARMUP_ENV: &str = "CAMERA_SENSOR_WARMUP_FRAMES";

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

/// Capture mechanism ("sub-driver"). All are nokhwa backends; one per OS camera API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverKind {
    /// nokhwa picks the native backend for this OS.
    Auto,
    /// Video4Linux2 (Linux).
    NokhwaV4l,
    /// AVFoundation (macOS).
    NokhwaAvFoundation,
    /// Media Foundation (Windows).
    NokhwaMsmf,
}

/// Which camera mode to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// The largest the camera offers (at its best frame rate for that size).
    Max,
    /// Exactly this size; the camera must offer it.
    Exact { width: u32, height: u32 },
}

/// Everything the camera driver needs besides the shared settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraOptions {
    pub driver: DriverKind,
    pub resolution: Resolution,
    /// Frames read and thrown away before the real one, so auto-exposure can settle.
    pub warmup_frames: u32,
}

impl Default for CameraOptions {
    fn default() -> Self {
        Self { driver: DriverKind::Auto, resolution: Resolution::Max, warmup_frames: 5 }
    }
}

impl CameraOptions {
    /// Read `CAMERA_SENSOR_DRIVER`, `_RESOLUTION`, `_WARMUP_FRAMES`; missing = default.
    pub fn from_env(deps: &Deps) -> Result<Self, ConfigError> {
        let mut options = Self::default();
        if let Some(value) = deps.env(DRIVER_ENV) {
            options.driver = parse_driver(&value)?;
        }
        if let Some(value) = deps.env(RESOLUTION_ENV) {
            options.resolution = parse_resolution(&value)?;
        }
        if let Some(value) = deps.env(WARMUP_ENV) {
            options.warmup_frames = parse_warmup(&value)?;
        }
        Ok(options)
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing (pure functions, unit tested)
// ---------------------------------------------------------------------------------------------

/// `auto` | `nokhwa-v4l` | `nokhwa-avfoundation` | `nokhwa-msmf`
pub fn parse_driver(value: &str) -> Result<DriverKind, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" | "nokhwa" => Ok(DriverKind::Auto),
        "nokhwa-v4l" => Ok(DriverKind::NokhwaV4l),
        "nokhwa-avfoundation" => Ok(DriverKind::NokhwaAvFoundation),
        "nokhwa-msmf" => Ok(DriverKind::NokhwaMsmf),
        _ => Err(invalid(DRIVER_ENV, value, "auto, nokhwa-v4l, nokhwa-avfoundation or nokhwa-msmf")),
    }
}

/// `max` | `<width>x<height>`, e.g. `1280x720`
pub fn parse_resolution(value: &str) -> Result<Resolution, ConfigError> {
    const EXPECTED: &str = "max or <width>x<height>, e.g. 1280x720";
    let value = value.trim().to_ascii_lowercase();
    if value == "max" {
        return Ok(Resolution::Max);
    }
    let Some((width, height)) = value.split_once('x') else {
        return Err(invalid(RESOLUTION_ENV, &value, EXPECTED));
    };
    match (width.parse::<u32>(), height.parse::<u32>()) {
        (Ok(width), Ok(height)) if width > 0 && height > 0 => Ok(Resolution::Exact { width, height }),
        _ => Err(invalid(RESOLUTION_ENV, &value, EXPECTED)),
    }
}

/// `0..=100`
pub fn parse_warmup(value: &str) -> Result<u32, ConfigError> {
    match value.trim().parse::<u32>() {
        Ok(frames @ 0..=100) => Ok(frames),
        _ => Err(invalid(WARMUP_ENV, value, "a number of frames 0..100")),
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
