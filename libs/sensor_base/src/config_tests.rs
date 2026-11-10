//! Unit tests for `config.rs` (setting parsers).

use super::*;

const KEY: &str = "X_DEVICE";

// --- device ---

#[test]
fn device_default_and_primary_alias() {
    assert_eq!(parse_device(KEY, "default"), Ok(DeviceSelector::Default));
    assert_eq!(parse_device(KEY, " PRIMARY "), Ok(DeviceSelector::Default));
}

#[test]
fn device_index() {
    assert_eq!(parse_device(KEY, "1"), Ok(DeviceSelector::Index(1)));
}

#[test]
fn device_name() {
    assert_eq!(parse_device(KEY, "name:HDMI-1"), Ok(DeviceSelector::Name("HDMI-1".into())));
}

#[test]
fn device_garbage_is_rejected_with_its_key() {
    let error = parse_device(KEY, "left").unwrap_err();
    assert_eq!(error.key, KEY);
    assert_eq!(error.value, "left");
}

// --- format ---

#[test]
fn format_png() {
    assert_eq!(parse_format(KEY, "png"), Ok(ImageFormat::Png));
}

#[test]
fn format_jpeg_has_default_quality() {
    assert_eq!(parse_format(KEY, "jpeg"), Ok(ImageFormat::Jpeg { quality: 85 }));
    assert_eq!(parse_format(KEY, "JPG"), Ok(ImageFormat::Jpeg { quality: 85 }));
}

#[test]
fn format_jpeg_with_quality() {
    assert_eq!(parse_format(KEY, "jpeg:60"), Ok(ImageFormat::Jpeg { quality: 60 }));
}

#[test]
fn format_jpeg_quality_out_of_range_is_rejected() {
    assert!(parse_format(KEY, "jpeg:0").is_err());
    assert!(parse_format(KEY, "jpeg:101").is_err());
    assert!(parse_format(KEY, "jpeg:high").is_err());
}

#[test]
fn format_unknown_is_rejected() {
    let error = parse_format("X_FORMAT", "bmp").unwrap_err();
    assert_eq!(error.to_string(), "X_FORMAT=\"bmp\" is invalid, expected png, jpeg or jpeg:<quality 1..100>");
    assert!(parse_format(KEY, "png:5").is_err());
}

// --- settings ---

#[test]
fn new_settings_are_default_device_png() {
    let settings = SensorSettings::new("cam-1");
    assert_eq!(settings.id(), "cam-1");
    assert_eq!(settings.device(), DeviceSelector::Default);
    assert_eq!(settings.format(), ImageFormat::Png);
}

#[test]
fn from_env_without_variables_uses_defaults() {
    let deps = Deps::in_process(&[]);
    let settings = SensorSettings::from_env(&deps, "SENSOR_BASE_TEST_UNSET", "screen").unwrap();
    assert_eq!(settings, SensorSettings::new("screen"));
}
