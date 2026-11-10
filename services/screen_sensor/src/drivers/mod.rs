//! Capture drivers: one file per mechanism, all implementing `domain::CaptureDriver`.
//!
//! To add a sub-driver (e.g. `windows-dxgi`): add a file here, a `DriverKind` case in `config.rs`,
//! and one `match` arm in `create`. `#[cfg(target_os = "...")]` ≈ C# `#if WINDOWS`.

// ---------------------------------------------------------------------------------------------
// Usings + modules
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use crate::config::DriverKind;
use crate::domain::CaptureDriver;

mod xcap_driver;

#[cfg(target_os = "linux")]
mod linux_x11;

#[cfg(not(target_os = "linux"))]
mod unsupported;

// ---------------------------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------------------------

/// Build the driver for `kind` (≈ a C# factory `switch` that returns `ICaptureDriver`).
/// Never fails: a driver that can't run on this OS reports `DriverNotSupported` when used,
/// so a misconfigured service still starts and answers with a clear error.
pub fn create(kind: DriverKind) -> Arc<dyn CaptureDriver> {
    match kind {
        DriverKind::Auto | DriverKind::Xcap => Arc::new(xcap_driver::XcapDriver),
        DriverKind::LinuxX11 => linux_x11_driver(),
    }
}

#[cfg(target_os = "linux")]
fn linux_x11_driver() -> Arc<dyn CaptureDriver> {
    Arc::new(linux_x11::LinuxX11Driver)
}

#[cfg(not(target_os = "linux"))]
fn linux_x11_driver() -> Arc<dyn CaptureDriver> {
    Arc::new(unsupported::UnsupportedDriver { name: "linux-x11" })
}
