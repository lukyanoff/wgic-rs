//! Unit tests for `config.rs` (driver setting).

use super::*;

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

#[test]
fn driver_defaults_to_auto() {
    assert_eq!(driver_from_env(&Deps::in_process(&[])), Ok(DriverKind::Auto));
}
