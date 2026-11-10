//! Driver `xcap`: the cross-platform `xcap` crate.
//! Windows: GDI/DXGI, macOS: CoreGraphics, Linux: X11 (xcb) or Wayland (portal / wlr-screencopy).
//! xcap picks the mechanism inside the OS itself; choose another driver to force one.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use xcap::Monitor;

use crate::config::ScreenSelector;
use crate::domain::{CaptureDriver, CaptureError, RawFrame, ScreenInfo, pick_screen};

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

/// Stateless: every capture lists monitors again, so plugging a monitor in needs no restart.
pub struct XcapDriver;

impl CaptureDriver for XcapDriver {
    fn name(&self) -> &'static str {
        "xcap"
    }

    fn capture(&self, screen: &ScreenSelector) -> Result<RawFrame, CaptureError> {
        let monitors = match Monitor::all() {
            Ok(monitors) => monitors,
            Err(error) => return Err(CaptureError::DriverUnavailable(error.to_string())),
        };
        let infos = screen_infos(&monitors);
        let index = pick_screen(&infos, screen)?;
        let monitor = &monitors[index];

        let pixels = match monitor.capture_image() {
            Ok(pixels) => pixels,
            Err(error) => return Err(CaptureError::CaptureFailed(error.to_string())),
        };
        Ok(RawFrame {
            width: pixels.width(),
            height: pixels.height(),
            rgba: pixels.into_raw(),
            screen_name: infos[index].name.clone(),
            screen_index: u32::try_from(index).unwrap_or(u32::MAX),
            scale_factor: monitor.scale_factor().unwrap_or(1.0),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// xcap getters return `Result`; a monitor whose name can't be read gets an empty name.
fn screen_infos(monitors: &[Monitor]) -> Vec<ScreenInfo> {
    let mut infos = Vec::new();
    for monitor in monitors {
        infos.push(ScreenInfo {
            name: monitor.name().unwrap_or_default(),
            is_primary: monitor.is_primary().unwrap_or(false),
        });
    }
    infos
}
