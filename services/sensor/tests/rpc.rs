//! Integration: the generated client → full connect stack → SensorService, in-process.
//! `FakeSource` stands in for the OS, so no screen or camera is needed.

use std::sync::Arc;

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::sensor::v1::{CaptureRequest, CaptureResponse, ListDevicesRequest, SensorServiceClient};
use kernel::{Channel, Deps};
use sensor::drivers::FakeSource;
use sensor::{ImageFormat, SensorSettings};

fn client(device: Option<u32>, format: ImageFormat, error: Option<ConnectError>) -> SensorServiceClient<Channel> {
    let settings = SensorSettings { id: "screen-1".into(), device, format };
    let module = sensor::module_with(settings, Arc::new(FakeSource { error }));
    let caller = Deps::in_process(&[module]);
    SensorServiceClient::new(caller.channel(), caller.client_config())
}

async fn capture(client: &SensorServiceClient<Channel>) -> Result<CaptureResponse, ConnectError> {
    Ok(client.capture(CaptureRequest::default()).await?.into_owned())
}

#[tokio::test]
async fn returns_png_and_metadata_of_the_default_device() {
    let response = capture(&client(None, ImageFormat::Png, None)).await.unwrap();
    assert!(response.image.starts_with(&[0x89, b'P', b'N', b'G']));
    let m = &response.metadata;
    assert_eq!((m.sensor_id.as_str(), m.kind.as_str(), m.mime_type.as_str()), ("screen-1", "screen", "image/png"));
    assert_eq!((m.device_name.as_str(), m.device_index, m.width, m.height), ("RIGHT", 1, 4, 2));
}

#[tokio::test]
async fn settings_choose_device_and_format() {
    let response = capture(&client(Some(0), ImageFormat::Jpeg, None)).await.unwrap();
    assert!(response.image.starts_with(&[0xFF, 0xD8]));
    assert_eq!((response.metadata.device_name.as_str(), response.metadata.mime_type.as_str()), ("LEFT", "image/jpeg"));
}

#[tokio::test]
async fn lists_devices() {
    let response = client(None, ImageFormat::Png, None).list_devices(ListDevicesRequest::default()).await.unwrap();
    let names: Vec<String> = response.into_owned().devices.into_iter().map(|d| d.name).collect();
    assert_eq!(names, ["LEFT", "RIGHT"]);
}

#[tokio::test]
async fn driver_errors_reach_the_client() {
    assert_eq!(capture(&client(Some(7), ImageFormat::Png, None)).await.unwrap_err().code, ErrorCode::NotFound);
    let no_display = Some(ConnectError::unavailable("no display"));
    assert_eq!(capture(&client(None, ImageFormat::Png, no_display)).await.unwrap_err().code, ErrorCode::Unavailable);
}
