//! Unit tests for `config.rs` (setting parsers).

use super::*;

// --- screen ---

#[test]
fn screen_primary() {
    assert_eq!(parse_screen("primary"), Ok(ScreenSelector::Primary));
    assert_eq!(parse_screen(" PRIMARY "), Ok(ScreenSelector::Primary));
}

#[test]
fn screen_index() {
    assert_eq!(parse_screen("1"), Ok(ScreenSelector::Index(1)));
}

#[test]
fn screen_name() {
    assert_eq!(parse_screen("name:HDMI-1"), Ok(ScreenSelector::Name("HDMI-1".into())));
}

#[test]
fn screen_garbage_is_rejected() {
    let error = parse_screen("left").unwrap_err();
    assert_eq!(error.key, SCREEN_ENV);
    assert_eq!(error.value, "left");
}

// --- format ---

#[test]
fn format_png() {
    assert_eq!(parse_format("png"), Ok(ImageFormat::Png));
}

#[test]
fn format_jpeg_has_default_quality() {
    assert_eq!(parse_format("jpeg"), Ok(ImageFormat::Jpeg { quality: 85 }));
    assert_eq!(parse_format("JPG"), Ok(ImageFormat::Jpeg { quality: 85 }));
}

#[test]
fn format_jpeg_with_quality() {
    assert_eq!(parse_format("jpeg:60"), Ok(ImageFormat::Jpeg { quality: 60 }));
}

#[test]
fn format_jpeg_quality_out_of_range_is_rejected() {
    assert!(parse_format("jpeg:0").is_err());
    assert!(parse_format("jpeg:101").is_err());
    assert!(parse_format("jpeg:high").is_err());
}

#[test]
fn format_unknown_is_rejected() {
    assert!(parse_format("bmp").is_err());
    assert!(parse_format("png:5").is_err());
}

// --- driver ---

#[test]
fn driver_names() {
    assert_eq!(parse_driver("auto"), Ok(DriverKind::Auto));
    assert_eq!(parse_driver("xcap"), Ok(DriverKind::Xcap));
    assert_eq!(parse_driver("Linux-X11"), Ok(DriverKind::LinuxX11));
}

#[test]
fn driver_unknown_is_rejected() {
    let error = parse_driver("gdi").unwrap_err();
    assert_eq!(error.to_string(), "SCREEN_SENSOR_DRIVER=\"gdi\" is invalid, expected auto, xcap or linux-x11");
}

// --- defaults ---

#[test]
fn defaults_are_primary_png_auto() {
    let config = SensorConfig::default();
    assert_eq!(config.screen(), ScreenSelector::Primary);
    assert_eq!(config.format(), ImageFormat::Png);
    assert_eq!(config.driver(), DriverKind::Auto);
}
