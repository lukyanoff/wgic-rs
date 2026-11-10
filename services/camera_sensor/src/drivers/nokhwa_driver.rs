//! Driver `nokhwa` / `nokhwa-<backend>`: webcams via the cross-platform `nokhwa` crate.
//!
//! Each capture opens the camera, throws away `warmup_frames` (auto-exposure settles),
//! keeps one frame and closes the camera again, so the camera light goes off between calls
//! and other apps can use the camera in between.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::collections::BTreeMap;

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraInfo, RequestedFormat, RequestedFormatType};
use nokhwa::{Camera, NokhwaError};
use sensor_base::{CaptureError, DeviceInfo, DeviceSelector, FrameSource, RawFrame, pick_device, rgb_to_rgba};

use super::KIND;
use crate::config::{CameraOptions, Resolution};

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

pub struct NokhwaDriver {
    pub name: &'static str,
    pub backend: ApiBackend,
    pub options: CameraOptions,
}

impl FrameSource for NokhwaDriver {
    fn name(&self) -> &'static str {
        self.name
    }

    fn kind(&self) -> &'static str {
        KIND
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, CaptureError> {
        Ok(device_infos(&self.cameras()?))
    }

    fn grab(&self, device: &DeviceSelector) -> Result<RawFrame, CaptureError> {
        let cameras = self.cameras()?;
        let devices = device_infos(&cameras);
        let index = pick_device(&devices, device)?;

        // The camera is released when `camera` goes out of scope (≈ C# `using var camera = ...`).
        let requested = RequestedFormat::new::<RgbFormat>(requested_type(self.options.resolution));
        let mut camera =
            Camera::with_backend(cameras[index].index().clone(), requested, self.backend).map_err(open_error)?;
        camera.open_stream().map_err(open_error)?;
        for _ in 0..self.options.warmup_frames {
            camera.frame().map_err(read_error)?;
        }
        let buffer = camera.frame().map_err(read_error)?;
        let rgb = buffer.decode_image::<RgbFormat>().map_err(read_error)?;

        let attributes = BTreeMap::from([
            ("native_format".to_string(), buffer.source_frame_format().to_string()),
            ("fps".to_string(), camera.frame_rate().to_string()),
        ]);
        Ok(RawFrame {
            rgba: rgb_to_rgba(rgb.as_raw()),
            width: rgb.width(),
            height: rgb.height(),
            device_name: devices[index].name.clone(),
            device_index: devices[index].index,
            attributes,
        })
    }
}

impl NokhwaDriver {
    /// Cameras that can actually deliver video, in a stable order (by OS device number).
    fn cameras(&self) -> Result<Vec<CameraInfo>, CaptureError> {
        let all = match nokhwa::query(self.backend) {
            Ok(cameras) => cameras,
            Err(error) => return Err(CaptureError::DriverUnavailable(error.to_string())),
        };
        let mut cameras = Vec::new();
        for camera in all {
            if can_capture_video(&camera) {
                cameras.push(camera);
            }
        }
        cameras.sort_by_key(|camera| camera.index().as_index().unwrap_or(u32::MAX));
        Ok(cameras)
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// The first camera in the list is the default one.
fn device_infos(cameras: &[CameraInfo]) -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    for (index, camera) in cameras.iter().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        devices.push(DeviceInfo { index, name: camera.human_name(), is_default: index == 0 });
    }
    devices
}

/// Linux lists every `/dev/videoN` node. A UVC webcam has two: the video node and a metadata
/// node that can't deliver pictures. Asking the node's V4L2 capabilities tells them apart without
/// streaming (the camera light stays off). If the question fails, keep the node.
#[cfg(target_os = "linux")]
fn can_capture_video(camera: &CameraInfo) -> bool {
    use v4l::capability::Flags;
    let Ok(number) = camera.index().as_index() else {
        return true;
    };
    match v4l::Device::new(number as usize).and_then(|device| device.query_caps()) {
        Ok(caps) => caps.capabilities.contains(Flags::VIDEO_CAPTURE),
        Err(_) => true,
    }
}

/// Other OSes list only real cameras.
#[cfg(not(target_os = "linux"))]
fn can_capture_video(_camera: &CameraInfo) -> bool {
    true
}

fn requested_type(resolution: Resolution) -> RequestedFormatType {
    match resolution {
        Resolution::Max => RequestedFormatType::AbsoluteHighestResolution,
        // Exactly that size, at the best frame rate the camera offers for it.
        Resolution::Exact { width, height } => {
            RequestedFormatType::HighestResolution(nokhwa::utils::Resolution::new(width, height))
        }
    }
}

/// Opening fails mostly for two reasons we can name: someone else has the camera, or we may not use it.
fn open_error(error: NokhwaError) -> CaptureError {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("busy") || lower.contains("in use") {
        return CaptureError::DeviceBusy(text);
    }
    if lower.contains("permission") || lower.contains("denied") {
        return CaptureError::DriverUnavailable(text);
    }
    CaptureError::CaptureFailed(text)
}

fn read_error(error: NokhwaError) -> CaptureError {
    CaptureError::CaptureFailed(error.to_string())
}

#[cfg(test)]
#[path = "nokhwa_driver_tests.rs"]
mod tests;
