//! `Screen`: monitors through the cross-platform `xcap` crate.
//! Windows: GDI/DXGI, macOS: CoreGraphics, Linux: X11 or Wayland (xcap picks the mechanism itself).

use connectrpc::ConnectError;
use contracts::proto::sensor::v1::DeviceInfo;
use xcap::Monitor;

use super::{FrameSource, RawFrame, device, pick_device};

/// Stateless: every call lists the monitors again, so plugging one in needs no restart.
pub struct Screen;

impl FrameSource for Screen {
    fn kind(&self) -> &'static str {
        "screen"
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, ConnectError> {
        Ok(devices(&monitors()?))
    }

    fn grab(&self, wanted: Option<u32>) -> Result<RawFrame, ConnectError> {
        let monitors = monitors()?;
        let devices = devices(&monitors);
        let position = pick_device(&devices, wanted)?;
        let pixels = match monitors[position].capture_image() {
            Ok(pixels) => pixels,
            Err(error) => return Err(ConnectError::internal(format!("screen capture failed: {error}"))),
        };
        let (width, height) = (pixels.width(), pixels.height());
        Ok(RawFrame { rgba: pixels.into_raw(), width, height, device: devices[position].clone() })
    }
}

/// No display (CI, containers, a locked session) is `unavailable`.
fn monitors() -> Result<Vec<Monitor>, ConnectError> {
    match Monitor::all() {
        Ok(monitors) => Ok(monitors),
        Err(error) => Err(ConnectError::unavailable(format!("no screen: {error}"))),
    }
}

/// xcap's getters can fail one by one; a name that can't be read stays empty.
fn devices(monitors: &[Monitor]) -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    for (index, monitor) in monitors.iter().enumerate() {
        devices.push(device(index, monitor.name().unwrap_or_default(), monitor.is_primary().unwrap_or(false)));
    }
    devices
}
