//! Integration: the real generated client → full connect stack → ScreenSensorService, in-process.
//! A fake driver stands in for the OS, so no real screen is needed. Logs and metrics are captured.

use std::sync::Arc;

use connectrpc::ErrorCode;
use contracts::proto::screen_sensor::v1::{CaptureRequest, CaptureResponse, ScreenSensorServiceClient};
use kernel::testing::{MemoryLogs, MemoryMetrics};
use kernel::{Channel, Deps, FieldValue};
use screen_sensor::{
    CaptureDriver, CaptureError, DriverKind, ImageFormat, RawFrame, ScreenInfo, ScreenSelector, SensorConfig,
    pick_screen,
};

// ---------------------------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------------------------

/// Two fake 4x2 monitors; `error` (if set) is returned instead of a frame.
struct FakeDriver {
    error: Option<CaptureError>,
}

impl CaptureDriver for FakeDriver {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn capture(&self, screen: &ScreenSelector) -> Result<RawFrame, CaptureError> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let screens = [
            ScreenInfo { name: "LEFT".into(), is_primary: false },
            ScreenInfo { name: "RIGHT".into(), is_primary: true },
        ];
        let index = pick_screen(&screens, screen)?;
        let name = screens[index].name.clone();
        Ok(RawFrame {
            rgba: vec![0; 4 * 2 * 4],
            width: 4,
            height: 2,
            screen_name: name,
            screen_index: index as u32,
            scale_factor: 2.0,
        })
    }
}

struct Setup {
    client: ScreenSensorServiceClient<Channel>,
    logs: MemoryLogs,
    metrics: MemoryMetrics,
}

fn setup(config: SensorConfig, error: Option<CaptureError>) -> Setup {
    let logs = MemoryLogs::new();
    let metrics = MemoryMetrics::new();
    let service_deps = Deps::in_process(&[]).with_logs(logs.clone()).with_metrics(metrics.clone());
    let module = screen_sensor::module_with(&service_deps, Arc::new(config), Arc::new(FakeDriver { error }));
    let caller = Deps::in_process(&[module]);
    let client = ScreenSensorServiceClient::new(caller.channel(), caller.client_config());
    Setup { client, logs, metrics }
}

fn config(screen: ScreenSelector, format: ImageFormat) -> SensorConfig {
    SensorConfig { screen, format, driver: DriverKind::Auto }
}

async fn capture(s: &Setup) -> CaptureResponse {
    s.client.capture(CaptureRequest::default()).await.unwrap().into_owned()
}

async fn capture_error_code(error: CaptureError) -> ErrorCode {
    let s = setup(SensorConfig::default(), Some(error));
    s.client.capture(CaptureRequest::default()).await.err().unwrap().code
}

// ---------------------------------------------------------------------------------------------
// Success
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn returns_png_and_metadata_of_primary_screen() {
    let s = setup(SensorConfig::default(), None);
    let response = capture(&s).await;

    assert!(response.image.starts_with(&[0x89, b'P', b'N', b'G']));
    let m = &response.metadata;
    assert_eq!((m.width, m.height), (4, 2));
    assert_eq!((m.format.as_str(), m.mime_type.as_str()), ("png", "image/png"));
    assert_eq!((m.screen_name.as_str(), m.screen_index), ("RIGHT", 1));
    assert_eq!(m.scale_factor, 2.0);
    assert_eq!(m.driver, "fake");
    assert!(m.captured_at_unix_ms > 0);
    assert_eq!(m.file_size_bytes, response.image.len() as u64);
    assert!(m.base64_size_bytes > m.file_size_bytes);
}

#[tokio::test]
async fn config_chooses_screen_and_format() {
    let s = setup(config(ScreenSelector::Name("LEFT".into()), ImageFormat::Jpeg { quality: 70 }), None);
    let response = capture(&s).await;

    assert!(response.image.starts_with(&[0xFF, 0xD8]));
    assert_eq!(response.metadata.mime_type, "image/jpeg");
    assert_eq!(response.metadata.screen_name, "LEFT");
}

#[tokio::test]
async fn logs_and_counts_each_capture() {
    let s = setup(SensorConfig::default(), None);
    capture(&s).await;

    let entry = s.logs.find("screen captured").unwrap();
    assert_eq!(entry.field("driver"), Some(&FieldValue::Str("fake".into())));
    assert_eq!(s.metrics.sum("screen_sensor.captures"), 1.0);
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn unknown_screen_is_not_found() {
    let s = setup(config(ScreenSelector::Index(7), ImageFormat::Png), None);
    let error = s.client.capture(CaptureRequest::default()).await.err().unwrap();
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(error.message.as_deref(), Some("screen 7 not found"));
}

#[tokio::test]
async fn error_codes_per_failure() {
    assert_eq!(
        capture_error_code(CaptureError::DriverNotSupported { driver: "x" }).await,
        ErrorCode::FailedPrecondition
    );
    assert_eq!(capture_error_code(CaptureError::DriverUnavailable("no display".into())).await, ErrorCode::Unavailable);
    assert_eq!(capture_error_code(CaptureError::CaptureFailed("boom".into())).await, ErrorCode::Internal);
}

#[tokio::test]
async fn failures_are_logged_and_counted_by_reason() {
    let s = setup(SensorConfig::default(), Some(CaptureError::DriverUnavailable("no display".into())));
    s.client.capture(CaptureRequest::default()).await.err().unwrap();

    let entry = s.logs.find("capture failed").unwrap();
    assert_eq!(entry.field("reason"), Some(&FieldValue::Str("driver_unavailable".into())));
    assert_eq!(s.metrics.sum("screen_sensor.failures"), 1.0);
    assert_eq!(s.metrics.sum("screen_sensor.captures"), 0.0);
}
