//! Integration: the real generated client → full connect stack → CameraSensorService, in-process.
//! `sensor_base::testing::FakeSource` stands in for the camera, so no device is needed.
//! (Logging/metrics details are covered once in `libs/sensor_base`.)

use std::sync::Arc;

use connectrpc::ErrorCode;
use contracts::proto::camera_sensor::v1::{
    CameraSensorServiceClient, CaptureRequest, CaptureResponse, ListDevicesRequest,
};
use kernel::testing::MemoryMetrics;
use kernel::{Channel, Deps};
use sensor_base::testing::FakeSource;
use sensor_base::{CaptureError, DeviceSelector, ImageFormat, SensorSettings};

// ---------------------------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------------------------

struct Setup {
    client: CameraSensorServiceClient<Channel>,
    metrics: MemoryMetrics,
}

fn setup(settings: SensorSettings, source: FakeSource) -> Setup {
    let metrics = MemoryMetrics::new();
    let service_deps = Deps::in_process(&[]).with_metrics(metrics.clone());
    let module = camera_sensor::module_with(&service_deps, Arc::new(settings), Arc::new(source));
    let caller = Deps::in_process(&[module]);
    let client = CameraSensorServiceClient::new(caller.channel(), caller.client_config());
    Setup { client, metrics }
}

async fn capture(s: &Setup) -> CaptureResponse {
    s.client.capture(CaptureRequest::default()).await.unwrap().into_owned()
}

async fn error_code(error: CaptureError) -> ErrorCode {
    let s = setup(SensorSettings::new("cam-1"), FakeSource::failing("camera", error));
    s.client.capture(CaptureRequest::default()).await.err().unwrap().code
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn returns_jpeg_and_metadata_of_chosen_camera() {
    let settings = SensorSettings {
        id: "cam-2".into(),
        device: DeviceSelector::Index(0),
        format: ImageFormat::Jpeg { quality: 80 },
    };
    let s = setup(settings, FakeSource::new("camera"));
    let response = capture(&s).await;

    assert!(response.image.starts_with(&[0xFF, 0xD8]));
    let m = &response.metadata;
    assert_eq!((m.sensor_id.as_str(), m.kind.as_str()), ("cam-2", "camera"));
    assert_eq!((m.device_name.as_str(), m.device_index), ("LEFT", 0));
    assert_eq!(s.metrics.sum("camera_sensor.captures"), 1.0);
}

#[tokio::test]
async fn lists_cameras() {
    let s = setup(SensorSettings::new("cam-1"), FakeSource::new("camera"));
    let response = s.client.list_devices(ListDevicesRequest::default()).await.unwrap().into_owned();
    assert_eq!(response.devices.len(), 2);
}

#[tokio::test]
async fn busy_camera_is_resource_exhausted() {
    assert_eq!(error_code(CaptureError::DeviceBusy("in use".into())).await, ErrorCode::ResourceExhausted);
}

#[tokio::test]
async fn no_camera_is_unavailable() {
    assert_eq!(error_code(CaptureError::DriverUnavailable("no camera".into())).await, ErrorCode::Unavailable);
}

#[tokio::test]
async fn backend_for_another_os_is_failed_precondition() {
    let error = CaptureError::DriverNotSupported { driver: "nokhwa-msmf" };
    assert_eq!(error_code(error).await, ErrorCode::FailedPrecondition);
}
