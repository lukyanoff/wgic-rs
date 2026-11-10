//! The sensor "base class". No proto types; the only IO is behind the injected `FrameSource`.
//!
//! Rust has no inheritance, so the C# pattern
//! `abstract class SensorBase { protected abstract Frame Grab(); public Capture Capture() {...} }`
//! becomes composition:
//! - `FrameSource` — the abstract part (≈ the `abstract` methods). Each service implements it per driver.
//! - `Sensor`      — the shared, concrete part (≈ the base class body). Holds a `FrameSource`.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::fmt;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, RgbaImage};

use crate::config::{DeviceSelector, ImageFormat, SensorConfig};

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// The selected device does not exist.
    DeviceNotFound { device: String },
    /// The configured driver cannot run on this OS (e.g. `linux-x11` on Windows).
    DriverNotSupported { driver: &'static str },
    /// The driver has nothing to talk to (no display, no camera, no permission, ...).
    DriverUnavailable(String),
    /// The device is in use by another program (typical for cameras).
    DeviceBusy(String),
    /// The OS call failed while taking the picture.
    CaptureFailed(String),
    /// Pixels could not be encoded to the requested format.
    EncodeFailed(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::DeviceNotFound { device } => write!(f, "device {device} not found"),
            CaptureError::DriverNotSupported { driver } => write!(f, "driver {driver} is not supported on this OS"),
            CaptureError::DriverUnavailable(why) => write!(f, "capture driver unavailable: {why}"),
            CaptureError::DeviceBusy(why) => write!(f, "device busy: {why}"),
            CaptureError::CaptureFailed(why) => write!(f, "capture failed: {why}"),
            CaptureError::EncodeFailed(why) => write!(f, "image encoding failed: {why}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The abstract part: what every driver must provide
// ---------------------------------------------------------------------------------------------

/// Uncompressed pixels from a driver: `width * height * 4` bytes, RGBA, row by row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub device_name: String,
    pub device_index: u32,
    /// Kind-specific details, e.g. `scale_factor` (screen), `native_format`/`fps` (camera).
    pub attributes: BTreeMap<String, String>,
}

/// One device a driver can capture from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Position in the driver's list; what `DeviceSelector::Index` refers to.
    pub index: u32,
    pub name: String,
    /// The device `DeviceSelector::Default` picks (primary monitor, first camera).
    pub is_default: bool,
}

/// One capture mechanism (≈ C# `interface IFrameSource`, or the abstract members of `SensorBase`).
/// Blocking: callers run it off the async threads.
pub trait FrameSource: Send + Sync {
    /// Stable driver id, e.g. `"xcap"`, `"linux-x11"`, `"nokhwa-v4l"`.
    fn name(&self) -> &'static str;
    /// Sensor kind: `"screen"` or `"camera"`.
    fn kind(&self) -> &'static str;
    fn list_devices(&self) -> Result<Vec<DeviceInfo>, CaptureError>;
    fn grab(&self, device: &DeviceSelector) -> Result<RawFrame, CaptureError>;
}

/// Position of the device `selector` points to. `Default` falls back to the first device.
pub fn pick_device(devices: &[DeviceInfo], selector: &DeviceSelector) -> Result<usize, CaptureError> {
    let not_found = || CaptureError::DeviceNotFound { device: selector.to_string() };
    if devices.is_empty() {
        return Err(not_found());
    }
    // `position` ≈ C# `FindIndex`, but returns Option<usize> instead of -1.
    let found = match selector {
        DeviceSelector::Default => Some(devices.iter().position(|d| d.is_default).unwrap_or(0)),
        DeviceSelector::Index(index) if *index < devices.len() => Some(*index),
        DeviceSelector::Index(_) => None,
        DeviceSelector::Name(name) => devices.iter().position(|d| &d.name == name),
    };
    found.ok_or_else(not_found)
}

/// Cameras usually deliver RGB (3 bytes per pixel); `RawFrame` wants RGBA. Alpha = opaque.
pub fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for pixel in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    rgba
}

// ---------------------------------------------------------------------------------------------
// Result data
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureMetadata {
    pub sensor_id: String,
    pub kind: &'static str,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub device_name: String,
    pub device_index: u32,
    pub driver: &'static str,
    pub captured_at_unix_ms: i64,
    pub capture_duration_ms: u32,
    /// Encoded image size (the PNG/JPEG file).
    pub file_size_bytes: u64,
    /// Size once base64-encoded, as in a JSON / NATS reply.
    pub base64_size_bytes: u64,
    pub attributes: BTreeMap<String, String>,
}

/// Encoded image + what it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub image: Vec<u8>,
    pub metadata: CaptureMetadata,
}

// ---------------------------------------------------------------------------------------------
// The concrete part: the "base class"
// ---------------------------------------------------------------------------------------------

/// Takes pictures. Config and driver are injected through the constructor
/// (`Arc<dyn T>` ≈ a shared reference to an interface in C#).
pub struct Sensor {
    config: Arc<dyn SensorConfig>,
    source: Arc<dyn FrameSource>,
}

impl Sensor {
    pub fn new(config: Arc<dyn SensorConfig>, source: Arc<dyn FrameSource>) -> Self {
        Self { config, source }
    }

    pub fn id(&self) -> String {
        self.config.id()
    }

    pub fn driver_name(&self) -> &'static str {
        self.source.name()
    }

    pub fn list_devices(&self) -> Result<Vec<DeviceInfo>, CaptureError> {
        self.source.list_devices()
    }

    /// Grab the configured device and encode it. Blocking.
    pub fn capture(&self) -> Result<Capture, CaptureError> {
        let captured_at = SystemTime::now();
        let started = Instant::now();
        let frame = self.source.grab(&self.config.device())?;
        let capture_duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

        let format = self.config.format();
        let image = encode(&frame, format)?;
        let metadata = CaptureMetadata {
            sensor_id: self.config.id(),
            kind: self.source.kind(),
            width: frame.width,
            height: frame.height,
            format,
            device_name: frame.device_name,
            device_index: frame.device_index,
            driver: self.source.name(),
            captured_at_unix_ms: unix_ms(captured_at),
            capture_duration_ms,
            file_size_bytes: image.len() as u64,
            base64_size_bytes: base64_len(image.len()),
            attributes: frame.attributes,
        };
        Ok(Capture { image, metadata })
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// RGBA pixels → PNG or JPEG bytes. JPEG has no alpha channel, so it is dropped first.
fn encode(frame: &RawFrame, format: ImageFormat) -> Result<Vec<u8>, CaptureError> {
    let Some(pixels) = RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone()) else {
        return Err(CaptureError::EncodeFailed(format!(
            "driver returned {} bytes for a {}x{} frame",
            frame.rgba.len(),
            frame.width,
            frame.height
        )));
    };
    let mut bytes = Vec::new();
    let result = match format {
        ImageFormat::Png => pixels.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png),
        ImageFormat::Jpeg { quality } => {
            let rgb = DynamicImage::ImageRgba8(pixels).to_rgb8();
            JpegEncoder::new_with_quality(&mut bytes, quality).encode_image(&rgb)
        }
    };
    match result {
        Ok(()) => Ok(bytes),
        Err(error) => Err(CaptureError::EncodeFailed(error.to_string())),
    }
}

/// Base64 turns every 3 bytes (rounded up) into 4 characters.
fn base64_len(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(3) * 4
}

fn unix_ms(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(since_epoch) => i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
