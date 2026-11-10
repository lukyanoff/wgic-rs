//! Proto ⇄ domain. Keep this thin: run the capture off the async threads, log/measure, encode.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use connectrpc::{ConnectError, ErrorCode, RequestContext, Response, ServiceRequest, ServiceResult};
use contracts::proto::screen_sensor::v1::{CaptureRequest, CaptureResponse, ImageMetadata, ScreenSensorService};
use kernel::{Deps, Logger, Metrics, kv};

use crate::domain::{Capture, CaptureError, ScreenSensor};

// ---------------------------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------------------------

/// Dependencies are injected through `new` (like constructor injection in C#).
pub struct ScreenSensorHandler {
    sensor: Arc<ScreenSensor>,
    log: Logger,
    metrics: Metrics,
}

impl ScreenSensorHandler {
    pub fn new(deps: &Deps, sensor: Arc<ScreenSensor>) -> Self {
        Self { sensor, log: deps.logger("screen_sensor"), metrics: deps.metrics("screen_sensor") }
    }

    /// Screen capture is blocking OS IO, so it runs on tokio's blocking thread pool
    /// (≈ C# `await Task.Run(() => sensor.Capture())`).
    async fn capture_off_thread(&self) -> Result<Capture, CaptureError> {
        let sensor = self.sensor.clone();
        match tokio::task::spawn_blocking(move || sensor.capture()).await {
            Ok(result) => result,
            Err(join_error) => Err(CaptureError::CaptureFailed(format!("capture task failed: {join_error}"))),
        }
    }
}

#[allow(refining_impl_trait)]
impl ScreenSensorService for ScreenSensorHandler {
    async fn capture(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, CaptureRequest>,
    ) -> ServiceResult<CaptureResponse> {
        let driver = self.sensor.driver_name();
        let capture = match self.capture_off_thread().await {
            Ok(capture) => capture,
            Err(error) => {
                self.log.warn("capture failed", [kv("reason", reason(&error)), kv("error", error.to_string())]);
                self.metrics.count("failures", 1, [kv("reason", reason(&error)), kv("driver", driver)]);
                return Err(to_connect_error(error));
            }
        };
        let m = &capture.metadata;
        self.log.info(
            "screen captured",
            [
                kv("driver", driver),
                kv("screen", m.screen_name.as_str()),
                kv("format", m.format.name()),
                kv("bytes", i64::try_from(capture.image.len()).unwrap_or(i64::MAX)),
            ],
        );
        self.metrics.count("captures", 1, [kv("driver", driver)]);
        self.metrics.record("capture_ms", f64::from(m.capture_duration_ms), [kv("driver", driver)]);
        Response::ok(to_response(capture))
    }
}

// ---------------------------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------------------------

fn to_response(capture: Capture) -> CaptureResponse {
    let m = capture.metadata;
    let metadata = ImageMetadata {
        width: m.width,
        height: m.height,
        format: m.format.name().into(),
        mime_type: m.format.mime_type().into(),
        screen_name: m.screen_name,
        screen_index: m.screen_index,
        scale_factor: m.scale_factor,
        driver: m.driver.into(),
        captured_at_unix_ms: m.captured_at_unix_ms,
        capture_duration_ms: m.capture_duration_ms,
        file_size_bytes: m.file_size_bytes,
        base64_size_bytes: m.base64_size_bytes,
        ..Default::default()
    };
    CaptureResponse { image: capture.image, metadata: metadata.into(), ..Default::default() }
}

fn to_connect_error(error: CaptureError) -> ConnectError {
    let code = match &error {
        CaptureError::ScreenNotFound { .. } => ErrorCode::NotFound,
        CaptureError::DriverNotSupported { .. } => ErrorCode::FailedPrecondition,
        CaptureError::DriverUnavailable(_) => ErrorCode::Unavailable,
        CaptureError::CaptureFailed(_) | CaptureError::EncodeFailed(_) => ErrorCode::Internal,
    };
    ConnectError::new(code, error.to_string())
}

/// Short, stable label for logs and the `screen_sensor.failures` metric.
fn reason(error: &CaptureError) -> &'static str {
    match error {
        CaptureError::ScreenNotFound { .. } => "screen_not_found",
        CaptureError::DriverNotSupported { .. } => "driver_not_supported",
        CaptureError::DriverUnavailable(_) => "driver_unavailable",
        CaptureError::CaptureFailed(_) => "capture_failed",
        CaptureError::EncodeFailed(_) => "encode_failed",
    }
}
