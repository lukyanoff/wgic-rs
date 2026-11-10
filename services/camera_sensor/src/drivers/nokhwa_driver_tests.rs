//! Unit tests for `nokhwa_driver.rs` (pure helpers only; capturing needs a real camera).

use super::*;

#[test]
fn busy_camera_is_device_busy() {
    let error = open_error(NokhwaError::OpenStreamError("Device or resource busy".into()));
    assert!(matches!(error, CaptureError::DeviceBusy(_)));
}

#[test]
fn permission_problem_is_unavailable() {
    let error = open_error(NokhwaError::OpenDeviceError("0".into(), "Permission denied".into()));
    assert!(matches!(error, CaptureError::DriverUnavailable(_)));
}

#[test]
fn other_open_problems_are_capture_failures() {
    let error = open_error(NokhwaError::GeneralError("no format".into()));
    assert!(matches!(error, CaptureError::CaptureFailed(_)));
}

#[test]
fn max_resolution_asks_for_highest() {
    assert_eq!(requested_type(Resolution::Max), RequestedFormatType::AbsoluteHighestResolution);
}

#[test]
fn exact_resolution_asks_for_that_size() {
    let requested = requested_type(Resolution::Exact { width: 1280, height: 720 });
    assert_eq!(requested, RequestedFormatType::HighestResolution(nokhwa::utils::Resolution::new(1280, 720)));
}
