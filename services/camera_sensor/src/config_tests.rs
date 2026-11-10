//! Unit tests for `config.rs` (camera-only settings).

use super::*;

#[test]
fn driver_names() {
    assert_eq!(parse_driver("auto"), Ok(DriverKind::Auto));
    assert_eq!(parse_driver("nokhwa"), Ok(DriverKind::Auto));
    assert_eq!(parse_driver("nokhwa-v4l"), Ok(DriverKind::NokhwaV4l));
    assert_eq!(parse_driver("Nokhwa-AVFoundation"), Ok(DriverKind::NokhwaAvFoundation));
    assert_eq!(parse_driver("nokhwa-msmf"), Ok(DriverKind::NokhwaMsmf));
}

#[test]
fn driver_unknown_is_rejected() {
    let error = parse_driver("opencv").unwrap_err();
    assert_eq!(error.key, DRIVER_ENV);
}

#[test]
fn resolution_max_and_exact() {
    assert_eq!(parse_resolution("max"), Ok(Resolution::Max));
    assert_eq!(parse_resolution("1280x720"), Ok(Resolution::Exact { width: 1280, height: 720 }));
    assert_eq!(parse_resolution(" 640X480 "), Ok(Resolution::Exact { width: 640, height: 480 }));
}

#[test]
fn resolution_garbage_is_rejected() {
    assert!(parse_resolution("hd").is_err());
    assert!(parse_resolution("1280x").is_err());
    assert!(parse_resolution("0x720").is_err());
}

#[test]
fn warmup_frames_range() {
    assert_eq!(parse_warmup("0"), Ok(0));
    assert_eq!(parse_warmup("10"), Ok(10));
    assert!(parse_warmup("101").is_err());
    assert!(parse_warmup("-1").is_err());
}

#[test]
fn defaults_are_auto_max_five_warmup_frames() {
    let options = CameraOptions::from_env(&Deps::in_process(&[])).unwrap();
    assert_eq!(options, CameraOptions::default());
    assert_eq!(options.warmup_frames, 5);
}
