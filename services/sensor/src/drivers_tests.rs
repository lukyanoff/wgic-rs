//! Unit tests for `drivers.rs`.

use connectrpc::ErrorCode;

use super::*;

fn devices() -> Vec<DeviceInfo> {
    vec![device(0, "LEFT".into(), false), device(1, "RIGHT".into(), true)]
}

#[test]
fn default_device_is_the_one_marked_default() {
    assert_eq!(pick_device(&devices(), None).unwrap(), 1);
}

#[test]
fn device_by_index() {
    assert_eq!(pick_device(&devices(), Some(0)).unwrap(), 0);
}

#[test]
fn missing_device_is_not_found() {
    let error = pick_device(&devices(), Some(7)).unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(error.message.as_deref(), Some("device 7 not found"));
    assert_eq!(pick_device(&[], None).unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn kinds_pick_their_driver() {
    assert_eq!(by_kind("camera").unwrap().kind(), "camera");
    assert_eq!(by_kind("screen").unwrap().kind(), "screen");
    assert!(by_kind("radar").is_none());
}
