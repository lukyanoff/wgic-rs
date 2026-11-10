//! Capture rules. No proto types; the only IO is behind the injected `CaptureDriver`.
//!
//! - `CaptureDriver` — the driver interface (≈ C# `ICaptureDriver`); implementations live in `drivers/`
//! - `ScreenSensor`  — the service class: asks the driver for raw pixels, encodes them, builds metadata
//! - `pick_screen`   — shared rule every driver uses to turn a `ScreenSelector` into a monitor index

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::fmt;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, RgbaImage};

use crate::config::{ImageFormat, ScreenSelector, ScreenSensorConfig};

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// The selected screen does not exist.
    ScreenNotFound { screen: String },
    /// The configured driver cannot run on this OS (e.g. `linux-x11` on Windows).
    DriverNotSupported { driver: &'static str },
    /// The driver exists but has nothing to talk to (no display, no permission, ...).
    DriverUnavailable(String),
    /// The OS call failed while taking the picture.
    CaptureFailed(String),
    /// Pixels could not be encoded to the requested format.
    EncodeFailed(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::ScreenNotFound { screen } => write!(f, "screen {screen} not found"),
            CaptureError::DriverNotSupported { driver } => write!(f, "driver {driver} is not supported on this OS"),
            CaptureError::DriverUnavailable(why) => write!(f, "capture driver unavailable: {why}"),
            CaptureError::CaptureFailed(why) => write!(f, "screen capture failed: {why}"),
            CaptureError::EncodeFailed(why) => write!(f, "image encoding failed: {why}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Driver interface + the data it returns
// ---------------------------------------------------------------------------------------------

/// Uncompressed pixels from a driver: `width * height * 4` bytes, RGBA, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct RawFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub screen_name: String,
    pub screen_index: u32,
    pub scale_factor: f32,
}

/// One capture mechanism (≈ C# `interface ICaptureDriver`). Blocking: callers run it off the async threads.
pub trait CaptureDriver: Send + Sync {
    /// Stable driver id, e.g. `"xcap"` or `"linux-x11"`.
    fn name(&self) -> &'static str;
    fn capture(&self, screen: &ScreenSelector) -> Result<RawFrame, CaptureError>;
}

/// What a driver knows about one monitor, enough to apply a `ScreenSelector`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenInfo {
    pub name: String,
    pub is_primary: bool,
}

/// Index of the monitor `selector` points to. `Primary` falls back to the first monitor.
pub fn pick_screen(screens: &[ScreenInfo], selector: &ScreenSelector) -> Result<usize, CaptureError> {
    let not_found = || CaptureError::ScreenNotFound { screen: selector.to_string() };
    if screens.is_empty() {
        return Err(not_found());
    }
    // `position` ≈ C# `FindIndex`, but returns Option<usize> instead of -1.
    let found = match selector {
        ScreenSelector::Primary => Some(screens.iter().position(|s| s.is_primary).unwrap_or(0)),
        ScreenSelector::Index(index) if *index < screens.len() => Some(*index),
        ScreenSelector::Index(_) => None,
        ScreenSelector::Name(name) => screens.iter().position(|s| &s.name == name),
    };
    found.ok_or_else(not_found)
}

// ---------------------------------------------------------------------------------------------
// Result data
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureMetadata {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub screen_name: String,
    pub screen_index: u32,
    pub scale_factor: f32,
    pub driver: &'static str,
    pub captured_at_unix_ms: i64,
    pub capture_duration_ms: u32,
    /// Encoded image size (the PNG/JPEG file).
    pub file_size_bytes: u64,
    /// Size once base64-encoded, as in a JSON / NATS reply.
    pub base64_size_bytes: u64,
}

/// Encoded image + what it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub image: Vec<u8>,
    pub metadata: CaptureMetadata,
}

// ---------------------------------------------------------------------------------------------
// The service class
// ---------------------------------------------------------------------------------------------

/// Takes pictures. Config and driver are injected through the constructor
/// (`Arc<dyn T>` ≈ a shared reference to an interface in C#).
pub struct ScreenSensor {
    config: Arc<dyn ScreenSensorConfig>,
    driver: Arc<dyn CaptureDriver>,
}

impl ScreenSensor {
    pub fn new(config: Arc<dyn ScreenSensorConfig>, driver: Arc<dyn CaptureDriver>) -> Self {
        Self { config, driver }
    }

    pub fn driver_name(&self) -> &'static str {
        self.driver.name()
    }

    /// Capture the configured screen and encode it. Blocking.
    pub fn capture(&self) -> Result<Capture, CaptureError> {
        let captured_at = SystemTime::now();
        let started = Instant::now();
        let frame = self.driver.capture(&self.config.screen())?;
        let capture_duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

        let format = self.config.format();
        let image = encode(&frame, format)?;
        let metadata = CaptureMetadata {
            width: frame.width,
            height: frame.height,
            format,
            screen_name: frame.screen_name,
            screen_index: frame.screen_index,
            scale_factor: frame.scale_factor,
            driver: self.driver.name(),
            captured_at_unix_ms: unix_ms(captured_at),
            capture_duration_ms,
            file_size_bytes: image.len() as u64,
            base64_size_bytes: base64_len(image.len()),
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
