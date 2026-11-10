//! Screen-only settings. Shared ones (`_ID`, `_DEVICE`, `_FORMAT`) come from `sensor_base::SensorSettings`.

use kernel::Deps;
use sensor_base::{ConfigError, invalid};

/// Env prefix for all screen settings: `SCREEN_SENSOR_ID`, `SCREEN_SENSOR_DEVICE`, ...
pub const ENV_PREFIX: &str = "SCREEN_SENSOR";
/// Instance id when `SCREEN_SENSOR_ID` is not set.
pub const DEFAULT_ID: &str = "screen";
pub const DRIVER_ENV: &str = "SCREEN_SENSOR_DRIVER";

/// Preferred capture mechanism ("driver/sub-driver"). Several can exist on one OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverKind {
    /// Let the service choose (currently `Xcap`).
    Auto,
    /// `xcap` crate: Windows, macOS, Linux (X11 + Wayland); picks the OS mechanism itself.
    Xcap,
    /// Native X11 via `x11rb` (Linux only).
    LinuxX11,
}

/// `SCREEN_SENSOR_DRIVER`, default `auto`.
pub fn driver_from_env(deps: &Deps) -> Result<DriverKind, ConfigError> {
    match deps.env(DRIVER_ENV) {
        Some(value) => parse_driver(&value),
        None => Ok(DriverKind::Auto),
    }
}

/// `auto` | `xcap` | `linux-x11`
pub fn parse_driver(value: &str) -> Result<DriverKind, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(DriverKind::Auto),
        "xcap" => Ok(DriverKind::Xcap),
        "linux-x11" => Ok(DriverKind::LinuxX11),
        _ => Err(invalid(DRIVER_ENV, value, "auto, xcap or linux-x11")),
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
