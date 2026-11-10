//! Settings, read once at startup: `SENSOR_ID`, `SENSOR_DEVICE`, `SENSOR_FORMAT` (`SENSOR_KIND` is read in `lib.rs`).

use kernel::Deps;

/// One instance's settings (≈ a C# options class).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorSettings {
    /// Instance id, e.g. `screen-1`. Default: the kind (`screen` / `camera`).
    pub id: String,
    /// Position in `ListDevices`; `None` = the default device (primary monitor, first camera).
    pub device: Option<u32>,
    pub format: ImageFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    /// Quality 85: a fraction of the PNG size, and still sharp text.
    Jpeg,
}

impl SensorSettings {
    pub fn from_env(deps: &Deps, kind: &str) -> Self {
        Self {
            id: deps.env("SENSOR_ID").unwrap_or_else(|| kind.to_string()),
            device: deps.setting("SENSOR_DEVICE", None, "a device index: 0, 1, ...", parse_device),
            format: deps.setting("SENSOR_FORMAT", ImageFormat::Png, "png or jpeg", parse_format),
        }
    }
}

impl ImageFormat {
    pub fn mime_type(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
        }
    }
}

/// `"1"` → `Some(Some(1))`; `"default"` → `Some(None)`; anything else is invalid (`None`).
fn parse_device(value: &str) -> Option<Option<u32>> {
    if value == "default" {
        return Some(None);
    }
    value.parse().ok().map(Some)
}

fn parse_format(value: &str) -> Option<ImageFormat> {
    match value.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpeg" | "jpg" => Some(ImageFormat::Jpeg),
        _ => None,
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
