//! E2E: SensorService in the real `host` binary, over HTTP and over NATS.
//!
//! The host uses *its own* screen and camera. Without them (CI, containers) the driver answers `unavailable`
//! or `not_found`, and a camera another app holds gives `resource_exhausted`: each still proves the RPC works.

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::sensor::v1::{CaptureRequest, CaptureResponse, ListDevicesRequest, SensorServiceClient};
use e2e::Host;
use kernel::{Channel, Deps};

fn client(deps: &Deps) -> SensorServiceClient<Channel> {
    SensorServiceClient::new(deps.channel(), deps.client_config())
}

async fn capture(deps: &Deps) -> Result<CaptureResponse, ConnectError> {
    Ok(client(deps).capture(CaptureRequest::default()).await?.into_owned())
}

/// A real PNG of `kind`, or one of the "no usable device here" codes.
fn assert_png_or_no_device(result: Result<CaptureResponse, ConnectError>, kind: &str) {
    match result {
        Ok(response) => {
            assert!(response.image.starts_with(&[0x89, b'P', b'N', b'G']));
            assert_eq!(response.metadata.kind, kind);
            assert!(response.metadata.width > 0 && response.metadata.height > 0);
        }
        Err(error) => {
            let expected = [ErrorCode::Unavailable, ErrorCode::NotFound, ErrorCode::ResourceExhausted];
            assert!(expected.contains(&error.code), "unexpected error: {error:?}");
        }
    }
}

#[tokio::test]
async fn screen_captures_over_http() {
    let host = Host::start("sensor").await;
    assert_png_or_no_device(capture(&host.http_deps()).await, "screen");
}

#[tokio::test]
async fn screen_captures_over_nats() {
    let host = Host::start("sensor").await;
    assert_png_or_no_device(capture(&host.nats_deps().await).await, "screen");
}

#[tokio::test]
async fn camera_captures_over_nats() {
    let host = Host::start_with_env("sensor", &[("SENSOR_KIND", "camera")]).await;
    assert_png_or_no_device(capture(&host.nats_deps().await).await, "camera");
}

#[tokio::test]
async fn camera_lists_devices_over_nats() {
    let host = Host::start_with_env("sensor", &[("SENSOR_KIND", "camera")]).await;
    match client(&host.nats_deps().await).list_devices(ListDevicesRequest::default()).await {
        Ok(response) => assert!(response.into_owned().devices.iter().all(|d| !d.name.is_empty())),
        Err(error) => assert_eq!(error.code, ErrorCode::Unavailable, "unexpected error: {error:?}"),
    }
}
