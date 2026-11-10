//! E2E: ScreenSensorService in the real `host` binary, over HTTP and over NATS.
//!
//! The host captures *its own* screen. Where there is no display (CI, containers) the
//! driver reports `unavailable`; both outcomes prove the RPC works end to end.

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::screen_sensor::v1::{CaptureRequest, CaptureResponse, ScreenSensorServiceClient};
use e2e::Host;
use kernel::Deps;

async fn capture(deps: &Deps) -> Result<CaptureResponse, ConnectError> {
    let client = ScreenSensorServiceClient::new(deps.channel(), deps.client_config());
    match client.capture(CaptureRequest::default()).await {
        Ok(response) => Ok(response.into_owned()),
        Err(error) => Err(error),
    }
}

/// A real PNG with metadata, or `unavailable` when the machine has no screen.
fn assert_png_or_no_display(result: Result<CaptureResponse, ConnectError>) {
    match result {
        Ok(response) => {
            assert!(response.image.starts_with(&[0x89, b'P', b'N', b'G']));
            assert!(response.metadata.width > 0 && response.metadata.height > 0);
            assert_eq!(response.metadata.driver, "xcap");
            assert_eq!(response.metadata.kind, "screen");
        }
        Err(error) => assert_eq!(error.code, ErrorCode::Unavailable, "unexpected error: {error:?}"),
    }
}

#[tokio::test]
async fn captures_over_http() {
    let host = Host::start("screen_sensor").await;
    assert_png_or_no_display(capture(&host.http_deps()).await);
}

#[tokio::test]
async fn captures_over_nats() {
    let host = Host::start("screen_sensor").await;
    let deps = host.nats_deps().await;
    assert_png_or_no_display(capture(&deps).await);
}
