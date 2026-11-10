//! Integration: the real generated client → full connect stack → ScreenSensorService, in-process.
//! `sensor_base::testing::FakeSource` stands in for the OS, so no real screen is needed.
//! (Logging/metrics details are covered once in `libs/sensor_base`.)

use std::sync::Arc;

use connectrpc::ErrorCode;
use contracts::proto::screen_sensor::v1::{
    CaptureRequest, CaptureResponse, ListDevicesRequest, ScreenSensorServiceClient,
};
use kernel::testing::MemoryMetrics;
use kernel::{Channel, Deps};
use sensor_base::testing::FakeSource;
use sensor_base::{CaptureError, DeviceSelector, ImageFormat, SensorSettings};

// ---------------------------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------------------------

struct Setup {
    client: ScreenSensorServiceClient<Channel>,
    metrics: MemoryMetrics,
}

fn setup(settings: SensorSettings, source: FakeSource) -> Setup {
    let metrics = MemoryMetrics::new();
    let service_deps = Deps::in_process(&[]).with_metrics(metrics.clone());
    let module = screen_sensor::module_with(&service_deps, Arc::new(settings), Arc::new(source));
    let caller = Deps::in_process(&[module]);
    let client = ScreenSensorServiceClient::new(caller.channel(), caller.client_config());
    Setup { client, metrics }
}

fn settings(device: DeviceSelector, format: ImageFormat) -> SensorSettings {
    SensorSettings { id: "screen-left".into(), device, format }
}

async fn capture(s: &Setup) -> CaptureResponse {
    s.client.capture(CaptureRequest::default()).await.unwrap().into_owned()
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn returns_png_and_metadata_of_default_screen() {
    let s = setup(settings(DeviceSelector::Default, ImageFormat::Png), FakeSource::new("screen"));
    let response = capture(&s).await;

    assert!(response.image.starts_with(&[0x89, b'P', b'N', b'G']));
    let m = &response.metadata;
    assert_eq!((m.sensor_id.as_str(), m.kind.as_str()), ("screen-left", "screen"));
    assert_eq!((m.device_name.as_str(), m.device_index), ("RIGHT", 1));
    assert_eq!(m.file_size_bytes, response.image.len() as u64);
    assert_eq!(s.metrics.sum("screen_sensor.captures"), 1.0);
}

#[tokio::test]
async fn config_chooses_device_and_format() {
    let config = settings(DeviceSelector::Name("LEFT".into()), ImageFormat::Jpeg { quality: 70 });
    let response = capture(&setup(config, FakeSource::new("screen"))).await;

    assert!(response.image.starts_with(&[0xFF, 0xD8]));
    assert_eq!(response.metadata.mime_type, "image/jpeg");
    assert_eq!(response.metadata.device_name, "LEFT");
}

#[tokio::test]
async fn lists_devices() {
    let s = setup(SensorSettings::new("screen"), FakeSource::new("screen"));
    let response = s.client.list_devices(ListDevicesRequest::default()).await.unwrap().into_owned();
    let names: Vec<&str> = response.devices.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["LEFT", "RIGHT"]);
}

#[tokio::test]
async fn unknown_screen_is_not_found() {
    let s = setup(settings(DeviceSelector::Index(7), ImageFormat::Png), FakeSource::new("screen"));
    let error = s.client.capture(CaptureRequest::default()).await.err().unwrap();
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(error.message.as_deref(), Some("device 7 not found"));
}

#[tokio::test]
async fn no_display_is_unavailable() {
    let source = FakeSource::failing("screen", CaptureError::DriverUnavailable("no display".into()));
    let s = setup(SensorSettings::new("screen"), source);
    let error = s.client.capture(CaptureRequest::default()).await.err().unwrap();
    assert_eq!(error.code, ErrorCode::Unavailable);
}
