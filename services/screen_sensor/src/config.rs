//! What to capture and how: the configuration injected into `ScreenSensor::new`.
//!
//! `ScreenSensorConfig` is the interface (≈ C# `ISensorOptions`), `SensorConfig` the plain implementation
//! (≈ a POCO options class). `SensorConfig::from_env` fills it from environment variables.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::fmt;

use kernel::Deps;

// ---------------------------------------------------------------------------------------------
// Data: the three settings
// ---------------------------------------------------------------------------------------------

/// Which screen (monitor) to capture. A Rust `enum` can carry data per case,
/// like a C# discriminated-union / sealed record hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenSelector {
    /// The OS primary monitor (first monitor if the OS marks none).
    Primary,
    /// Position in the driver's monitor list, starting at 0.
    Index(usize),
    /// Monitor name as the driver reports it, e.g. `HDMI-1`.
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

// ---------------------------------------------------------------------------------------------
// Interface
// ---------------------------------------------------------------------------------------------

/// The configuration contract (≈ C# `interface IScreenSensorConfig`).
/// `Send + Sync` = safe to share between threads, like a thread-safe singleton.
pub trait ScreenSensorConfig: Send + Sync {
    fn screen(&self) -> ScreenSelector;
    fn format(&self) -> ImageFormat;
    fn driver(&self) -> DriverKind;
}

// ---------------------------------------------------------------------------------------------
// Implementation: plain values (+ reading them from env)
// ---------------------------------------------------------------------------------------------

/// Plain settings object. Defaults: primary screen, PNG, auto driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorConfig {
    pub screen: ScreenSelector,
    pub format: ImageFormat,
    pub driver: DriverKind,
}

impl Default for SensorConfig {
    fn default() -> Self {
        Self { screen: ScreenSelector::Primary, format: ImageFormat::Png, driver: DriverKind::Auto }
    }
}

impl ScreenSensorConfig for SensorConfig {
    fn screen(&self) -> ScreenSelector {
        self.screen.clone()
    }

    fn format(&self) -> ImageFormat {
        self.format
    }

    fn driver(&self) -> DriverKind {
        self.driver
    }
}

/// A setting that could not be parsed. Reported at startup.
#[derive(Debug, PartialEq, Eq)]
pub struct ConfigError {
    pub key: &'static str,
    pub value: String,
    pub expected: &'static str,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={:?} is invalid, expected {}", self.key, self.value, self.expected)
    }
}

pub const SCREEN_ENV: &str = "SCREEN_SENSOR_SCREEN";
pub const FORMAT_ENV: &str = "SCREEN_SENSOR_FORMAT";
pub const DRIVER_ENV: &str = "SCREEN_SENSOR_DRIVER";

impl SensorConfig {
    /// Read `SCREEN_SENSOR_SCREEN`, `SCREEN_SENSOR_FORMAT`, `SCREEN_SENSOR_DRIVER`; missing = default.
    pub fn from_env(deps: &Deps) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        if let Some(value) = deps.env(SCREEN_ENV) {
            config.screen = parse_screen(&value)?;
        }
        if let Some(value) = deps.env(FORMAT_ENV) {
            config.format = parse_format(&value)?;
        }
        if let Some(value) = deps.env(DRIVER_ENV) {
            config.driver = parse_driver(&value)?;
        }
        Ok(config)
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing (pure functions, unit tested)
// ---------------------------------------------------------------------------------------------

/// `primary` | `<index>` | `name:<NAME>`
pub fn parse_screen(value: &str) -> Result<ScreenSelector, ConfigError> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("primary") {
        return Ok(ScreenSelector::Primary);
    }
    if let Some(name) = value.strip_prefix("name:") {
        return Ok(ScreenSelector::Name(name.to_string()));
    }
    match value.parse::<usize>() {
        Ok(index) => Ok(ScreenSelector::Index(index)),
        Err(_) => Err(invalid(SCREEN_ENV, value, "primary, a monitor index (0, 1, ...) or name:<NAME>")),
    }
}

/// `png` | `jpeg` | `jpeg:<1..=100>` (`jpg` works too). JPEG default quality is 85.
pub fn parse_format(value: &str) -> Result<ImageFormat, ConfigError> {
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
            _ => Err(invalid(FORMAT_ENV, &value, EXPECTED)),
        },
        _ => Err(invalid(FORMAT_ENV, &value, EXPECTED)),
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

fn invalid(key: &'static str, value: &str, expected: &'static str) -> ConfigError {
    ConfigError { key, value: value.to_string(), expected }
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

impl fmt::Display for ScreenSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScreenSelector::Primary => write!(f, "primary"),
            ScreenSelector::Index(index) => write!(f, "{index}"),
            ScreenSelector::Name(name) => write!(f, "name:{name}"),
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
