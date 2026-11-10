//! `Camera`: webcams through the cross-platform `nokhwa` crate (it picks the OS camera API itself).
//!
//! Each capture opens the camera, drops a few warm-up frames (auto-exposure settles), keeps one and closes the
//! camera again: the camera light goes off between calls and other apps can use it. A call takes about 1–2 s.

use std::sync::Arc;

use connectrpc::ConnectError;
use contracts::proto::sensor::v1::DeviceInfo;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraInfo, RequestedFormat, RequestedFormatType};
use nokhwa::{Camera as NokhwaCamera, NokhwaError};

use super::{FrameSource, RawFrame, device, pick_device};

/// Frames read and thrown away before the real one.
const WARMUP_FRAMES: u32 = 5;

pub struct Camera;

/// The camera driver. macOS asks the user for camera access once, before the first use.
pub fn camera() -> Arc<dyn FrameSource> {
    #[cfg(target_os = "macos")]
    nokhwa::nokhwa_initialize(|_granted| {});
    Arc::new(Camera)
}

impl FrameSource for Camera {
    fn kind(&self) -> &'static str {
        "camera"
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, ConnectError> {
        Ok(devices(&cameras()?))
    }

    fn grab(&self, wanted: Option<u32>) -> Result<RawFrame, ConnectError> {
        let cameras = cameras()?;
        let devices = devices(&cameras);
        let position = pick_device(&devices, wanted)?;

        // The largest size the camera offers. The camera closes when `camera` goes out of scope
        // (≈ C# `using var camera = ...`).
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
        let index = cameras[position].index().clone();
        let mut camera = NokhwaCamera::with_backend(index, requested, ApiBackend::Auto).map_err(open_error)?;
        camera.open_stream().map_err(open_error)?;
        for _ in 0..WARMUP_FRAMES {
            camera.frame().map_err(read_error)?;
        }
        let rgb = camera.frame().and_then(|frame| frame.decode_image::<RgbFormat>()).map_err(read_error)?;
        let (width, height) = (rgb.width(), rgb.height());
        Ok(RawFrame { rgba: rgb_to_rgba(rgb.as_raw()), width, height, device: devices[position].clone() })
    }
}

/// Cameras that can deliver video, in a stable order (by OS device number). No camera API is `unavailable`.
fn cameras() -> Result<Vec<CameraInfo>, ConnectError> {
    let mut cameras = match nokhwa::query(ApiBackend::Auto) {
        Ok(cameras) => cameras,
        Err(error) => return Err(ConnectError::unavailable(format!("no camera: {error}"))),
    };
    cameras.retain(can_capture_video); // ≈ C# `RemoveAll(c => !CanCaptureVideo(c))`
    cameras.sort_by_key(|camera| camera.index().as_index().unwrap_or(u32::MAX));
    Ok(cameras)
}

/// The first camera is the default one.
fn devices(cameras: &[CameraInfo]) -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    for (index, camera) in cameras.iter().enumerate() {
        devices.push(device(index, camera.human_name(), index == 0));
    }
    devices
}

/// Linux lists every `/dev/videoN`, and a UVC webcam has two: the video node and a metadata node that can't
/// deliver pictures. The node's V4L2 capabilities tell them apart without streaming. If asking fails, keep it.
#[cfg(target_os = "linux")]
fn can_capture_video(camera: &CameraInfo) -> bool {
    let Ok(number) = camera.index().as_index() else {
        return true;
    };
    match v4l::Device::new(number as usize).and_then(|device| device.query_caps()) {
        Ok(caps) => caps.capabilities.contains(v4l::capability::Flags::VIDEO_CAPTURE),
        Err(_) => true,
    }
}

/// Other OSes list only real cameras.
#[cfg(not(target_os = "linux"))]
fn can_capture_video(_camera: &CameraInfo) -> bool {
    true
}

/// Opening fails mostly because another app has the camera (`resource_exhausted`) or we may not use it
/// (`unavailable`).
fn open_error(error: NokhwaError) -> ConnectError {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("busy") || lower.contains("in use") {
        return ConnectError::resource_exhausted(format!("camera busy: {text}"));
    }
    if lower.contains("permission") || lower.contains("denied") {
        return ConnectError::unavailable(format!("camera not allowed: {text}"));
    }
    ConnectError::internal(format!("camera open failed: {text}"))
}

fn read_error(error: NokhwaError) -> ConnectError {
    ConnectError::internal(format!("camera read failed: {error}"))
}

/// Cameras deliver RGB (3 bytes per pixel); `RawFrame` wants RGBA. Alpha = opaque.
fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for pixel in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    rgba
}

#[cfg(test)]
#[path = "camera_tests.rs"]
mod tests;
