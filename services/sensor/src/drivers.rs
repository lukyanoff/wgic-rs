//! The capture mechanisms. `FrameSource` is the interface (≈ C# `interface IFrameSource`); each file in
//! `drivers/` is one implementation:
//! - `Screen` (xcap): Windows, macOS, Linux X11 + Wayland
//! - `Camera` (nokhwa): V4L2 on Linux, AVFoundation on macOS, Media Foundation on Windows
//! - `FakeSource`: two fake devices, for tests

mod camera;
mod fake;
mod screen;

use std::sync::Arc;

use connectrpc::ConnectError;
use contracts::proto::sensor::v1::DeviceInfo;

pub use fake::FakeSource;
pub use screen::Screen;

// ---------------------------------------------------------------------------------------------
// Interface
// ---------------------------------------------------------------------------------------------

/// One way to take pictures. Its methods block (OS calls), so the handler runs them on a worker thread.
pub trait FrameSource: Send + Sync {
    /// `"screen"` or `"camera"`.
    fn kind(&self) -> &'static str;
    fn list_devices(&self) -> Result<Vec<DeviceInfo>, ConnectError>;
    /// One frame from `device` (an index in `list_devices`), or from the default device when `None`.
    fn grab(&self, device: Option<u32>) -> Result<RawFrame, ConnectError>;
}

/// Uncompressed pixels: `width * height * 4` bytes, RGBA, row by row.
pub struct RawFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub device: DeviceInfo,
}

// ---------------------------------------------------------------------------------------------
// Factory and shared helpers
// ---------------------------------------------------------------------------------------------

/// `SENSOR_KIND` → driver (≈ a factory `switch`). `None` = unknown kind.
pub fn by_kind(kind: &str) -> Option<Arc<dyn FrameSource>> {
    match kind {
        "screen" => Some(screen()),
        "camera" => Some(camera::camera()),
        _ => None,
    }
}

/// The default driver.
pub fn screen() -> Arc<dyn FrameSource> {
    Arc::new(Screen)
}

/// The position of the wanted device in `devices`: `wanted`, or the default one (the first one when no device
/// says it is the default). An empty list or an index past the end is `not_found`.
pub fn pick_device(devices: &[DeviceInfo], wanted: Option<u32>) -> Result<usize, ConnectError> {
    let found = match wanted {
        Some(index) => devices.iter().position(|d| d.index == index),
        None if devices.is_empty() => None,
        // `position` ≈ C# `FindIndex`, but gives `None` instead of -1.
        None => Some(devices.iter().position(|d| d.is_default).unwrap_or(0)),
    };
    let Some(position) = found else {
        let name = match wanted {
            Some(index) => index.to_string(),
            None => "default".to_string(),
        };
        return Err(ConnectError::not_found(format!("device {name} not found")));
    };
    Ok(position)
}

/// `DeviceInfo` for the device at `index` (positions are small, so `as u32` can't overflow in practice).
pub fn device(index: usize, name: String, is_default: bool) -> DeviceInfo {
    DeviceInfo { index: index as u32, name, is_default, ..Default::default() }
}

#[cfg(test)]
#[path = "drivers_tests.rs"]
mod tests;
