//! Unit tests for `config.rs`.

use super::*;

#[test]
fn reads_settings_with_defaults() {
    let deps = Deps::in_process(&[]).with_env([("SENSOR_DEVICE", "1"), ("SENSOR_FORMAT", "JPG")]);
    let settings = SensorSettings::from_env(&deps, "camera");
    assert_eq!(settings, SensorSettings { id: "camera".into(), device: Some(1), format: ImageFormat::Jpeg });
}

#[test]
fn device_is_an_index_or_default() {
    assert_eq!(parse_device("default"), Some(None));
    assert_eq!(parse_device("2"), Some(Some(2)));
    assert_eq!(parse_device("HDMI-1"), None);
}

#[test]
#[should_panic(expected = "SENSOR_FORMAT=\"gif\" is invalid, expected png or jpeg")]
fn unknown_format_stops_the_host() {
    SensorSettings::from_env(&Deps::in_process(&[]).with_env([("SENSOR_FORMAT", "gif")]), "screen");
}
