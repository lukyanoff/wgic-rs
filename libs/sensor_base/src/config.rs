//! Settings every sensor has: instance id, which device, output format.
//!
//! `SensorConfig` is the interface injected into `Sensor::new` (≈ C# `ISensorOptions`),
//! `SensorSettings` the plain implementation (≈ a POCO options class) with `from_env`.
//! Kind-specific settings (driver, camera resolution, ...) live in each service.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::fmt;

use kernel::Deps;

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

/// Which device (monitor, camera) to capture. A Rust `enum` can carry data per case,
/// like a C# discriminated-union / sealed record hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelector {
    /// The driver's default device: primary monitor, first camera.
    Default,
    /// Position in the driver's device list (see the `ListDevices` RPC), starting at 0.
    Index(usize),
    /// Device name as the driver reports it, e.g. `HDMI-1` or `Integrated Camera`.
    Name(String),
}

/// Encoding of the returned image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    /// `quality` is 1..=100.
    Jpeg {
        quality: u8,
    },
}

// ---------------------------------------------------------------------------------------------
// Interface
// ---------------------------------------------------------------------------------------------

/// The configuration contract (≈ C# `interface ISensorConfig`).
/// `Send + Sync` = safe to share between threads, like a thread-safe singleton.
pub trait SensorConfig: Send + Sync {
    /// Instance id, e.g. `screen-left`. Shows up in metadata, logs and metrics.
    fn id(&self) -> String;
    fn device(&self) -> DeviceSelector;
    fn format(&self) -> ImageFormat;
}

// ---------------------------------------------------------------------------------------------
// Implementation: plain values (+ reading them from env)
// ---------------------------------------------------------------------------------------------

/// Plain settings object. Defaults: default device, PNG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorSettings {
    pub id: String,
    pub device: DeviceSelector,
    pub format: ImageFormat,
}

impl SensorSettings {
    /// Default device, PNG.
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string(), device: DeviceSelector::Default, format: ImageFormat::Png }
    }

    /// Read `<PREFIX>_ID`, `<PREFIX>_DEVICE`, `<PREFIX>_FORMAT`; a missing value keeps its default.
    /// Example: prefix `SCREEN_SENSOR` → `SCREEN_SENSOR_DEVICE=1`.
    pub fn from_env(deps: &Deps, prefix: &str, default_id: &str) -> Result<Self, ConfigError> {
        let mut settings = Self::new(default_id);
        if let Some(id) = deps.env(&format!("{prefix}_ID")) {
            settings.id = id.trim().to_string();
        }
        let device_key = format!("{prefix}_DEVICE");
        if let Some(value) = deps.env(&device_key) {
            settings.device = parse_device(&device_key, &value)?;
        }
        let format_key = format!("{prefix}_FORMAT");
        if let Some(value) = deps.env(&format_key) {
            settings.format = parse_format(&format_key, &value)?;
        }
        Ok(settings)
    }
}

impl SensorConfig for SensorSettings {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn device(&self) -> DeviceSelector {
        self.device.clone()
    }

    fn format(&self) -> ImageFormat {
        self.format
    }
}

/// A setting that could not be parsed. Reported at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub key: String,
    pub value: String,
    pub expected: &'static str,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={:?} is invalid, expected {}", self.key, self.value, self.expected)
    }
}

/// Build a `ConfigError`. Public so services can report their own settings the same way.
pub fn invalid(key: &str, value: &str, expected: &'static str) -> ConfigError {
    ConfigError { key: key.to_string(), value: value.to_string(), expected }
}

// ---------------------------------------------------------------------------------------------
// Parsing (pure functions, unit tested). `key` is only used in the error message.
// ---------------------------------------------------------------------------------------------

/// `default` (or `primary`) | `<index>` | `name:<NAME>`
pub fn parse_device(key: &str, value: &str) -> Result<DeviceSelector, ConfigError> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("default") || value.eq_ignore_ascii_case("primary") {
        return Ok(DeviceSelector::Default);
    }
    if let Some(name) = value.strip_prefix("name:") {
        return Ok(DeviceSelector::Name(name.to_string()));
    }
    match value.parse::<usize>() {
        Ok(index) => Ok(DeviceSelector::Index(index)),
        Err(_) => Err(invalid(key, value, "default, a device index (0, 1, ...) or name:<NAME>")),
    }
}

/// `png` | `jpeg` | `jpeg:<1..=100>` (`jpg` works too). JPEG default quality is 85.
pub fn parse_format(key: &str, value: &str) -> Result<ImageFormat, ConfigError> {
    const EXPECTED: &str = "png, jpeg or jpeg:<quality 1..100>";
    let value = value.trim().to_ascii_lowercase();
    // `split_once` ≈ C# `value.Split(':', 2)`, returns None when there is no ':'.
    let (kind, quality) = match value.split_once(':') {
        Some((kind, quality)) => (kind, Some(quality)),
        None => (value.as_str(), None),
    };
    match (kind, quality) {
        ("png", None) => Ok(ImageFormat::Png),
        ("jpeg" | "jpg", None) => Ok(ImageFormat::Jpeg { quality: 85 }),
        ("jpeg" | "jpg", Some(quality)) => match quality.parse::<u8>() {
            Ok(quality @ 1..=100) => Ok(ImageFormat::Jpeg { quality }),
            _ => Err(invalid(key, &value, EXPECTED)),
        },
        _ => Err(invalid(key, &value, EXPECTED)),
    }
}

// ---------------------------------------------------------------------------------------------
// Names (used in metadata, logs and metrics)
// ---------------------------------------------------------------------------------------------

impl ImageFormat {
    pub fn name(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg { .. } => "jpeg",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg { .. } => "image/jpeg",
        }
    }
}

impl fmt::Display for DeviceSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceSelector::Default => write!(f, "default"),
            DeviceSelector::Index(index) => write!(f, "{index}"),
            DeviceSelector::Name(name) => write!(f, "name:{name}"),
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
