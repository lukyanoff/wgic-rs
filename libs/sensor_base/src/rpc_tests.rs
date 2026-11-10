//! Unit tests for `rpc.rs`: mapping, logs and metrics, with the fake source.

use super::*;
use crate::config::SensorSettings;
use crate::testing::FakeSource;
use kernel::FieldValue;
use kernel::testing::{MemoryLogs, MemoryMetrics};

struct Setup {
    rpc: SensorRpc,
    logs: MemoryLogs,
    metrics: MemoryMetrics,
}

fn setup(source: FakeSource) -> Setup {
    let logs = MemoryLogs::new();
    let metrics = MemoryMetrics::new();
    let deps = Deps::in_process(&[]).with_logs(logs.clone()).with_metrics(metrics.clone());
    let sensor = Sensor::new(Arc::new(SensorSettings::new("cam-1")), Arc::new(source));
    Setup { rpc: SensorRpc::new(&deps, "camera_sensor", sensor), logs, metrics }
}

#[tokio::test]
async fn capture_fills_proto_metadata() {
    let reply = setup(FakeSource::new("camera")).rpc.capture().await.unwrap();
    let m = reply.metadata;
    assert_eq!((m.sensor_id.as_str(), m.kind.as_str(), m.driver.as_str()), ("cam-1", "camera", "fake"));
    assert_eq!((m.format.as_str(), m.mime_type.as_str()), ("png", "image/png"));
    assert_eq!(m.file_size_bytes, reply.image.len() as u64);
    assert_eq!(m.attributes.get("fake").map(String::as_str), Some("yes"));
}

#[tokio::test]
async fn capture_is_logged_and_counted_per_sensor() {
    let s = setup(FakeSource::new("camera"));
    s.rpc.capture().await.unwrap();

    let entry = s.logs.find("capture done").unwrap();
    assert_eq!(entry.field("sensor_id"), Some(&FieldValue::Str("cam-1".into())));
    assert_eq!(s.metrics.sum("camera_sensor.captures"), 1.0);
}

#[tokio::test]
async fn failure_is_logged_counted_and_mapped() {
    let s = setup(FakeSource::failing("camera", CaptureError::DeviceBusy("in use".into())));
    let error = s.rpc.capture().await.err().unwrap();

    assert_eq!(error.code, ErrorCode::ResourceExhausted);
    let entry = s.logs.find("capture failed").unwrap();
    assert_eq!(entry.field("reason"), Some(&FieldValue::Str("device_busy".into())));
    assert_eq!(s.metrics.sum("camera_sensor.failures"), 1.0);
}

#[tokio::test]
async fn list_devices_maps_to_proto() {
    let devices = setup(FakeSource::new("screen")).rpc.list_devices().await.unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!((devices[1].index, devices[1].name.as_str(), devices[1].is_default), (1, "RIGHT", true));
}

#[test]
fn error_codes_per_failure() {
    let code = |e: CaptureError| to_connect_error(e).code;
    assert_eq!(code(CaptureError::DeviceNotFound { device: "9".into() }), ErrorCode::NotFound);
    assert_eq!(code(CaptureError::DriverNotSupported { driver: "x" }), ErrorCode::FailedPrecondition);
    assert_eq!(code(CaptureError::DriverUnavailable("no display".into())), ErrorCode::Unavailable);
    assert_eq!(code(CaptureError::DeviceBusy("in use".into())), ErrorCode::ResourceExhausted);
    assert_eq!(code(CaptureError::CaptureFailed("boom".into())), ErrorCode::Internal);
    assert_eq!(code(CaptureError::EncodeFailed("boom".into())), ErrorCode::Internal);
}
