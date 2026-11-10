//! Driver `linux-x11`: native X11 capture with `x11rb` (pure Rust, talks the X protocol over a socket).
//!
//! Steps: connect to `$DISPLAY` → list monitors (RandR) → `GetImage` of that monitor's rectangle
//! on the root window → convert X's BGRX pixels to RGBA.
//! Works on X11 sessions (and XWayland, where it sees only X11 windows).

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::fmt::Display;

// `use Trait as _` brings extension methods into scope without naming the trait
// (≈ C# `using` a namespace for its extension methods).
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat, ImageOrder, Screen};
use x11rb::rust_connection::RustConnection;

use crate::config::ScreenSelector;
use crate::domain::{CaptureDriver, CaptureError, RawFrame, ScreenInfo, pick_screen};

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

pub struct LinuxX11Driver;

/// One monitor: its name/primary flag plus where it sits on the big X11 root window.
struct X11Monitor {
    info: ScreenInfo,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

/// Every plane: "give me all colour bits".
const ALL_PLANES: u32 = !0;

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

impl CaptureDriver for LinuxX11Driver {
    fn name(&self) -> &'static str {
        "linux-x11"
    }

    fn capture(&self, screen: &ScreenSelector) -> Result<RawFrame, CaptureError> {
        // Connection closes when `conn` goes out of scope (≈ C# `using var conn = ...`).
        let (conn, screen_num) = match x11rb::connect(None) {
            Ok(connected) => connected,
            Err(error) => return Err(CaptureError::DriverUnavailable(format!("cannot connect to X server: {error}"))),
        };
        let root_screen = &conn.setup().roots[screen_num];
        let monitors = list_monitors(&conn, root_screen);

        let mut infos = Vec::new();
        for monitor in &monitors {
            infos.push(monitor.info.clone());
        }
        let index = pick_screen(&infos, screen)?;
        let monitor = &monitors[index];

        let rgba = grab_rgba(&conn, root_screen, monitor)?;
        Ok(RawFrame {
            rgba,
            width: monitor.width.into(),
            height: monitor.height.into(),
            screen_name: monitor.info.name.clone(),
            screen_index: u32::try_from(index).unwrap_or(u32::MAX),
            scale_factor: 1.0, // X11 has no per-monitor scale
        })
    }
}

// ---------------------------------------------------------------------------------------------
// X11 calls
// ---------------------------------------------------------------------------------------------

/// Monitors from RandR 1.5. If RandR is missing (old/minimal X servers), the whole root window
/// is reported as one primary monitor.
fn list_monitors(conn: &RustConnection, root_screen: &Screen) -> Vec<X11Monitor> {
    let whole_screen = X11Monitor {
        info: ScreenInfo { name: "screen-0".into(), is_primary: true },
        x: 0,
        y: 0,
        width: root_screen.width_in_pixels,
        height: root_screen.height_in_pixels,
    };
    // `.ok()` turns Result into Option, dropping the error (we fall back instead).
    let Some(reply) = conn.randr_get_monitors(root_screen.root, true).ok().and_then(|c| c.reply().ok()) else {
        return vec![whole_screen];
    };
    let mut monitors = Vec::new();
    for m in reply.monitors {
        let info = ScreenInfo { name: atom_name(conn, m.name), is_primary: m.primary };
        monitors.push(X11Monitor { info, x: m.x, y: m.y, width: m.width, height: m.height });
    }
    if monitors.is_empty() {
        monitors.push(whole_screen);
    }
    monitors
}

/// RandR names monitors with atoms (interned strings); this looks the text up. Empty on error.
fn atom_name(conn: &RustConnection, atom: u32) -> String {
    match conn.get_atom_name(atom).ok().and_then(|c| c.reply().ok()) {
        Some(reply) => String::from_utf8_lossy(&reply.name).into_owned(),
        None => String::new(),
    }
}

/// Read the monitor's pixels from the root window and convert them to RGBA.
fn grab_rgba(conn: &RustConnection, root_screen: &Screen, monitor: &X11Monitor) -> Result<Vec<u8>, CaptureError> {
    let (x, y, width, height) = (monitor.x, monitor.y, monitor.width, monitor.height);
    let cookie = conn.get_image(ImageFormat::Z_PIXMAP, root_screen.root, x, y, width, height, ALL_PLANES);
    let reply = cookie.map_err(failed)?.reply().map_err(failed)?;

    let setup = conn.setup();
    let bits_per_pixel = setup.pixmap_formats.iter().find(|f| f.depth == reply.depth).map(|f| f.bits_per_pixel);
    if bits_per_pixel != Some(32) || setup.image_byte_order != ImageOrder::LSB_FIRST {
        return Err(CaptureError::CaptureFailed(format!(
            "unsupported X11 pixel format (depth {}, {bits_per_pixel:?} bits per pixel)",
            reply.depth
        )));
    }
    Ok(bgrx_to_rgba(&reply.data))
}

fn failed(error: impl Display) -> CaptureError {
    CaptureError::CaptureFailed(error.to_string())
}

// ---------------------------------------------------------------------------------------------
// Pixels (pure, unit tested)
// ---------------------------------------------------------------------------------------------

/// X11 32-bit little-endian pixels are stored B, G, R, unused. We need R, G, B, A (opaque).
pub(crate) fn bgrx_to_rgba(bgrx: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgrx.len());
    // `chunks_exact(4)` ≈ iterating the buffer 4 bytes at a time.
    for pixel in bgrx.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    rgba
}

#[cfg(test)]
#[path = "linux_x11_tests.rs"]
mod tests;
