//! Unit tests for `domain.rs`, with a fake driver (no real screen needed).

use super::*;
use crate::config::{DriverKind, SensorConfig};

// ---------------------------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------------------------

/// Returns a fixed 2x2 frame, or a fixed error (≈ a hand-written C# fake / stub).
struct FakeDriver {
    result: Result<RawFrame, CaptureError>,
}

impl CaptureDriver for FakeDriver {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn capture(&self, _screen: &ScreenSelector) -> Result<RawFrame, CaptureError> {
        self.result.clone()
    }
}

fn frame_2x2() -> RawFrame {
    RawFrame {
        rgba: vec![255; 2 * 2 * 4],
        width: 2,
        height: 2,
        screen_name: "TEST-1".into(),
        screen_index: 0,
        scale_factor: 1.5,
    }
}

fn sensor(format: ImageFormat, result: Result<RawFrame, CaptureError>) -> ScreenSensor {
    let config = SensorConfig { screen: ScreenSelector::Primary, format, driver: DriverKind::Auto };
    ScreenSensor::new(Arc::new(config), Arc::new(FakeDriver { result }))
}

fn screens() -> Vec<ScreenInfo> {
    vec![ScreenInfo { name: "eDP-1".into(), is_primary: false }, ScreenInfo { name: "HDMI-1".into(), is_primary: true }]
}

const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];
const JPEG_MAGIC: [u8; 2] = [0xFF, 0xD8];

// ---------------------------------------------------------------------------------------------
// ScreenSensor::capture
// ---------------------------------------------------------------------------------------------

#[test]
fn png_capture_starts_with_png_signature() {
    let capture = sensor(ImageFormat::Png, Ok(frame_2x2())).capture().unwrap();
    assert!(capture.image.starts_with(&PNG_MAGIC));
}

#[test]
fn jpeg_capture_starts_with_jpeg_signature() {
    let capture = sensor(ImageFormat::Jpeg { quality: 80 }, Ok(frame_2x2())).capture().unwrap();
    assert!(capture.image.starts_with(&JPEG_MAGIC));
}

#[test]
fn metadata_describes_the_frame() {
    let metadata = sensor(ImageFormat::Png, Ok(frame_2x2())).capture().unwrap().metadata;
    assert_eq!((metadata.width, metadata.height), (2, 2));
    assert_eq!(metadata.format, ImageFormat::Png);
    assert_eq!(metadata.screen_name, "TEST-1");
    assert_eq!(metadata.scale_factor, 1.5);
    assert_eq!(metadata.driver, "fake");
    assert!(metadata.captured_at_unix_ms > 0);
}

#[test]
fn metadata_has_file_and_base64_sizes() {
    let capture = sensor(ImageFormat::Png, Ok(frame_2x2())).capture().unwrap();
    let file_size = capture.image.len() as u64;
    assert_eq!(capture.metadata.file_size_bytes, file_size);
    assert_eq!(capture.metadata.base64_size_bytes, file_size.div_ceil(3) * 4);
}

#[test]
fn base64_length_rounds_up_to_whole_groups() {
    assert_eq!(base64_len(0), 0);
    assert_eq!(base64_len(1), 4);
    assert_eq!(base64_len(3), 4);
    assert_eq!(base64_len(4), 8);
    assert_eq!(base64_len(1_000_000), 1_333_336);
}

#[test]
fn driver_errors_are_passed_through() {
    let error = CaptureError::DriverUnavailable("no display".into());
    let result = sensor(ImageFormat::Png, Err(error.clone())).capture();
    assert_eq!(result, Err(error));
}

#[test]
fn wrong_pixel_buffer_size_is_encode_error() {
    let mut frame = frame_2x2();
    frame.rgba.pop();
    let result = sensor(ImageFormat::Png, Ok(frame)).capture();
    assert!(matches!(result, Err(CaptureError::EncodeFailed(_))));
}

// ---------------------------------------------------------------------------------------------
// pick_screen
// ---------------------------------------------------------------------------------------------

#[test]
fn primary_picks_the_primary_monitor() {
    assert_eq!(pick_screen(&screens(), &ScreenSelector::Primary), Ok(1));
}

#[test]
fn primary_falls_back_to_first_monitor() {
    let mut list = screens();
    list[1].is_primary = false;
    assert_eq!(pick_screen(&list, &ScreenSelector::Primary), Ok(0));
}

#[test]
fn index_and_name_pick_that_monitor() {
    assert_eq!(pick_screen(&screens(), &ScreenSelector::Index(0)), Ok(0));
    assert_eq!(pick_screen(&screens(), &ScreenSelector::Name("HDMI-1".into())), Ok(1));
}

#[test]
fn unknown_screen_is_not_found() {
    let error = pick_screen(&screens(), &ScreenSelector::Index(5)).unwrap_err();
    assert_eq!(error.to_string(), "screen 5 not found");
    assert!(pick_screen(&screens(), &ScreenSelector::Name("DP-9".into())).is_err());
    assert!(pick_screen(&[], &ScreenSelector::Primary).is_err());
}
