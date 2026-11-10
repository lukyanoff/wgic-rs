//! Camera capture drivers, all implementing `sensor_base::FrameSource`.
//!
//! Every sub-driver today is a nokhwa backend (V4L2, AVFoundation, Media Foundation), so one
//! `NokhwaDriver` struct covers them. A different mechanism (e.g. GStreamer) would get its own file.

use std::sync::Arc;

use nokhwa::utils::ApiBackend;
use sensor_base::{FrameSource, UnsupportedSource};

use crate::config::{CameraOptions, DriverKind};

mod nokhwa_driver;

/// Every driver here reports this kind in metadata.
const KIND: &str = "camera";

/// Build the driver for `options.driver` (≈ a C# factory `switch` that returns `IFrameSource`).
/// Never fails: a backend for another OS answers `failed_precondition` when used.
pub fn create(options: CameraOptions) -> Arc<dyn FrameSource> {
    // `cfg!(...)` is a compile-time true/false (≈ C# `#if LINUX` as an expression).
    let (name, backend, supported) = match options.driver {
        DriverKind::Auto => ("nokhwa", ApiBackend::Auto, true),
        DriverKind::NokhwaV4l => ("nokhwa-v4l", ApiBackend::Video4Linux, cfg!(target_os = "linux")),
        DriverKind::NokhwaAvFoundation => ("nokhwa-avfoundation", ApiBackend::AVFoundation, cfg!(target_os = "macos")),
        DriverKind::NokhwaMsmf => ("nokhwa-msmf", ApiBackend::MediaFoundation, cfg!(target_os = "windows")),
    };
    if !supported {
        return Arc::new(UnsupportedSource { name, kind: KIND });
    }
    Arc::new(nokhwa_driver::NokhwaDriver { name, backend, options })
}
