//! Unit tests for `camera.rs` (pure helpers only: capturing needs a real camera).

use connectrpc::ErrorCode;

use super::*;

#[test]
fn busy_camera_is_resource_exhausted() {
    let error = open_error(NokhwaError::OpenStreamError("Device or resource busy".into()));
    assert_eq!(error.code, ErrorCode::ResourceExhausted);
}

#[test]
fn permission_problem_is_unavailable() {
    let error = open_error(NokhwaError::OpenDeviceError("0".into(), "Permission denied".into()));
    assert_eq!(error.code, ErrorCode::Unavailable);
}

#[test]
fn other_open_problems_are_internal() {
    assert_eq!(open_error(NokhwaError::GeneralError("no format".into())).code, ErrorCode::Internal);
}

#[test]
fn rgb_gets_an_opaque_alpha() {
    assert_eq!(rgb_to_rgba(&[1, 2, 3, 4, 5, 6]), [1, 2, 3, 255, 4, 5, 6, 255]);
}
