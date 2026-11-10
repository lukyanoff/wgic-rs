//! Unit tests for `domain.rs`, with the fake source (no real device needed).

use super::*;
use crate::config::SensorSettings;
use crate::testing::FakeSource;

// ---------------------------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------------------------

fn sensor(format: ImageFormat, source: FakeSource) -> Sensor {
    let mut settings = SensorSettings::new("cam-1");
    settings.format = format;
    Sensor::new(Arc::new(settings), Arc::new(source))
}

fn devices() -> Vec<DeviceInfo> {
    vec![
        DeviceInfo { index: 0, name: "eDP-1".into(), is_default: false },
        DeviceInfo { index: 1, name: "HDMI-1".into(), is_default: true },
    ]
}

const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];
const JPEG_MAGIC: [u8; 2] = [0xFF, 0xD8];

// ---------------------------------------------------------------------------------------------
// Sensor::capture
// ---------------------------------------------------------------------------------------------

#[test]
fn png_capture_starts_with_png_signature() {
    let capture = sensor(ImageFormat::Png, FakeSource::new("camera")).capture().unwrap();
    assert!(capture.image.starts_with(&PNG_MAGIC));
}

#[test]
fn jpeg_capture_starts_with_jpeg_signature() {
    let capture = sensor(ImageFormat::Jpeg { quality: 80 }, FakeSource::new("camera")).capture().unwrap();
    assert!(capture.image.starts_with(&JPEG_MAGIC));
}

#[test]
fn metadata_describes_sensor_device_and_frame() {
    let m = sensor(ImageFormat::Png, FakeSource::new("camera")).capture().unwrap().metadata;
    assert_eq!((m.sensor_id.as_str(), m.kind, m.driver), ("cam-1", "camera", "fake"));
    assert_eq!((m.width, m.height), (4, 2));
    assert_eq!((m.device_name.as_str(), m.device_index), ("RIGHT", 1));
    assert_eq!(m.attributes.get("fake").map(String::as_str), Some("yes"));
    assert!(m.captured_at_unix_ms > 0);
}

#[test]
fn metadata_has_file_and_base64_sizes() {
    let capture = sensor(ImageFormat::Png, FakeSource::new("screen")).capture().unwrap();
    let file_size = capture.image.len() as u64;
    assert_eq!(capture.metadata.file_size_bytes, file_size);
    assert_eq!(capture.metadata.base64_size_bytes, file_size.div_ceil(3) * 4);
}

#[test]
fn source_errors_are_passed_through() {
    let error = CaptureError::DeviceBusy("in use".into());
    let result = sensor(ImageFormat::Png, FakeSource::failing("camera", error.clone())).capture();
    assert_eq!(result, Err(error));
}

#[test]
fn wrong_pixel_buffer_size_is_encode_error() {
    let frame = RawFrame { rgba: vec![0; 3], width: 1, height: 1, ..Default::default() };
    assert!(matches!(encode(&frame, ImageFormat::Png), Err(CaptureError::EncodeFailed(_))));
}

// ---------------------------------------------------------------------------------------------
// pick_device, rgb_to_rgba, base64_len
// ---------------------------------------------------------------------------------------------

#[test]
fn default_picks_the_default_device() {
    assert_eq!(pick_device(&devices(), &DeviceSelector::Default), Ok(1));
}

#[test]
fn default_falls_back_to_first_device() {
    let mut list = devices();
    list[1].is_default = false;
    assert_eq!(pick_device(&list, &DeviceSelector::Default), Ok(0));
}

#[test]
fn index_and_name_pick_that_device() {
    assert_eq!(pick_device(&devices(), &DeviceSelector::Index(0)), Ok(0));
    assert_eq!(pick_device(&devices(), &DeviceSelector::Name("HDMI-1".into())), Ok(1));
}

#[test]
fn unknown_device_is_not_found() {
    let error = pick_device(&devices(), &DeviceSelector::Index(5)).unwrap_err();
    assert_eq!(error.to_string(), "device 5 not found");
    assert!(pick_device(&devices(), &DeviceSelector::Name("DP-9".into())).is_err());
    assert!(pick_device(&[], &DeviceSelector::Default).is_err());
}

#[test]
fn rgb_becomes_opaque_rgba() {
    assert_eq!(rgb_to_rgba(&[1, 2, 3, 4, 5, 6]), vec![1, 2, 3, 255, 4, 5, 6, 255]);
}

#[test]
fn base64_length_rounds_up_to_whole_groups() {
    assert_eq!(base64_len(0), 0);
    assert_eq!(base64_len(1), 4);
    assert_eq!(base64_len(3), 4);
    assert_eq!(base64_len(4), 8);
    assert_eq!(base64_len(1_000_000), 1_333_336);
}
