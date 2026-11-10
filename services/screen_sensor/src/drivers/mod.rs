//! Screen capture drivers: one file per mechanism, all implementing `sensor_base::FrameSource`.
//!
//! To add a sub-driver (e.g. `windows-dxgi`): add a file here, a `DriverKind` case in `config.rs`,
//! and one `match` arm in `create`. `#[cfg(target_os = "...")]` ≈ C# `#if WINDOWS`.

// ---------------------------------------------------------------------------------------------
// Usings + modules
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use sensor_base::FrameSource;

use crate::config::DriverKind;

mod xcap_driver;

#[cfg(target_os = "linux")]
mod linux_x11;

/// Every driver here reports this kind in metadata.
const KIND: &str = "screen";

// ---------------------------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------------------------

/// Build the driver for `kind` (≈ a C# factory `switch` that returns `IFrameSource`).
/// Never fails: a driver that can't run on this OS answers `failed_precondition` when used,
/// so a misconfigured service still starts and gives a clear error.
pub fn create(kind: DriverKind) -> Arc<dyn FrameSource> {
    match kind {
        DriverKind::Auto | DriverKind::Xcap => Arc::new(xcap_driver::XcapDriver),
        DriverKind::LinuxX11 => linux_x11_driver(),
    }
}

#[cfg(target_os = "linux")]
fn linux_x11_driver() -> Arc<dyn FrameSource> {
    Arc::new(linux_x11::LinuxX11Driver)
}

#[cfg(not(target_os = "linux"))]
fn linux_x11_driver() -> Arc<dyn FrameSource> {
    Arc::new(sensor_base::UnsupportedSource { name: "linux-x11", kind: KIND })
}
