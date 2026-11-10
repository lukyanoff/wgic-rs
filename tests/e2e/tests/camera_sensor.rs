//! E2E: CameraSensorService in the real `host` binary, over HTTP and over NATS.
//!
//! The host uses *its own* camera. Without a camera (CI, containers) the driver reports
//! `unavailable` or `not_found`, and a camera used by another app gives `resource_exhausted`;
//! any of these proves the RPC works end to end.

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::camera_sensor::v1::{
    CameraSensorServiceClient, CaptureRequest, CaptureResponse, ListDevicesRequest,
};
use e2e::Host;
use kernel::Deps;

fn client(deps: &Deps) -> CameraSensorServiceClient<kernel::Channel> {
    CameraSensorServiceClient::new(deps.channel(), deps.client_config())
}

async fn capture(deps: &Deps) -> Result<CaptureResponse, ConnectError> {
    match client(deps).capture(CaptureRequest::default()).await {
        Ok(response) => Ok(response.into_owned()),
        Err(error) => Err(error),
    }
}

/// A real PNG with camera metadata, or one of the "no usable camera here" codes.
fn assert_png_or_no_camera(result: Result<CaptureResponse, ConnectError>) {
    match result {
        Ok(response) => {
            assert!(response.image.starts_with(&[0x89, b'P', b'N', b'G']));
            assert_eq!(response.metadata.kind, "camera");
            assert!(response.metadata.width > 0 && response.metadata.height > 0);
        }
        Err(error) => {
            let expected = [ErrorCode::Unavailable, ErrorCode::NotFound, ErrorCode::ResourceExhausted];
            assert!(expected.contains(&error.code), "unexpected error: {error:?}");
        }
    }
}

#[tokio::test]
async fn captures_over_http() {
    let host = Host::start("camera_sensor").await;
    assert_png_or_no_camera(capture(&host.http_deps()).await);
}

#[tokio::test]
async fn captures_over_nats() {
    let host = Host::start("camera_sensor").await;
    let deps = host.nats_deps().await;
    assert_png_or_no_camera(capture(&deps).await);
}

#[tokio::test]
async fn lists_devices_over_nats() {
    let host = Host::start("camera_sensor").await;
    let deps = host.nats_deps().await;
    match client(&deps).list_devices(ListDevicesRequest::default()).await {
        Ok(response) => assert!(response.into_owned().devices.iter().all(|d| !d.name.is_empty())),
        Err(error) => assert_eq!(error.code, ErrorCode::Unavailable, "unexpected error: {error:?}"),
    }
}
