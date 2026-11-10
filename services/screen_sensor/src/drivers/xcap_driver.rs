//! Driver `xcap`: the cross-platform `xcap` crate.
//! Windows: GDI/DXGI, macOS: CoreGraphics, Linux: X11 (xcb) or Wayland (portal / wlr-screencopy).
//! xcap picks the mechanism inside the OS itself; choose another driver to force one.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::collections::BTreeMap;

use sensor_base::{CaptureError, DeviceInfo, DeviceSelector, FrameSource, RawFrame, pick_device};
use xcap::Monitor;

use super::KIND;

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

/// Stateless: every call lists monitors again, so plugging a monitor in needs no restart.
pub struct XcapDriver;

impl FrameSource for XcapDriver {
    fn name(&self) -> &'static str {
        "xcap"
    }

    fn kind(&self) -> &'static str {
        KIND
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, CaptureError> {
        Ok(device_infos(&monitors()?))
    }

    fn grab(&self, device: &DeviceSelector) -> Result<RawFrame, CaptureError> {
        let monitors = monitors()?;
        let devices = device_infos(&monitors);
        let index = pick_device(&devices, device)?;
        let monitor = &monitors[index];

        let pixels = match monitor.capture_image() {
            Ok(pixels) => pixels,
            Err(error) => return Err(CaptureError::CaptureFailed(error.to_string())),
        };
        let scale_factor = monitor.scale_factor().unwrap_or(1.0);
        Ok(RawFrame {
            width: pixels.width(),
            height: pixels.height(),
            rgba: pixels.into_raw(),
            device_name: devices[index].name.clone(),
            device_index: devices[index].index,
            attributes: BTreeMap::from([("scale_factor".to_string(), scale_factor.to_string())]),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn monitors() -> Result<Vec<Monitor>, CaptureError> {
    match Monitor::all() {
        Ok(monitors) => Ok(monitors),
        Err(error) => Err(CaptureError::DriverUnavailable(error.to_string())),
    }
}

/// xcap getters return `Result`; a monitor whose name can't be read gets an empty name.
fn device_infos(monitors: &[Monitor]) -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    for (index, monitor) in monitors.iter().enumerate() {
        devices.push(DeviceInfo {
            index: u32::try_from(index).unwrap_or(u32::MAX),
            name: monitor.name().unwrap_or_default(),
            is_default: monitor.is_primary().unwrap_or(false),
        });
    }
    devices
}
