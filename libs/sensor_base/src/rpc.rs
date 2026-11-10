//! The shared RPC part of every sensor handler: run the capture off the async threads,
//! log/measure, map domain → `sensor.v1` proto and `CaptureError` → Connect code.
//!
//! A service's handler holds a `SensorRpc` and delegates to it (≈ calling `base.Capture()`
//! from an override in C#), then wraps the result in its own response message.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::sensor::v1 as proto;
use kernel::{Deps, Logger, Metrics, kv};

use crate::domain::{Capture, CaptureError, DeviceInfo, Sensor};

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

/// Encoded image + its `sensor.v1.ImageMetadata`, ready to put into a service's `CaptureResponse`.
pub struct CaptureReply {
    pub image: Vec<u8>,
    pub metadata: proto::ImageMetadata,
}

/// Dependencies are injected through `new` (like constructor injection in C#).
pub struct SensorRpc {
    sensor: Arc<Sensor>,
    log: Logger,
    metrics: Metrics,
}

// ---------------------------------------------------------------------------------------------
// Methods
// ---------------------------------------------------------------------------------------------

impl SensorRpc {
    /// `meter` names logs and metrics, usually the service name (`screen_sensor` → `screen_sensor.captures`).
    pub fn new(deps: &Deps, meter: &str, sensor: Sensor) -> Self {
        Self { sensor: Arc::new(sensor), log: deps.logger(meter), metrics: deps.metrics(meter) }
    }

    pub async fn capture(&self) -> Result<CaptureReply, ConnectError> {
        // Capturing is blocking OS IO, so it runs on tokio's blocking thread pool
        // (≈ C# `await Task.Run(() => sensor.Capture())`).
        let sensor = self.sensor.clone();
        let result = match tokio::task::spawn_blocking(move || sensor.capture()).await {
            Ok(result) => result,
            Err(join_error) => Err(CaptureError::CaptureFailed(format!("capture task failed: {join_error}"))),
        };
        match result {
            Ok(capture) => {
                self.record_success(&capture);
                Ok(to_reply(capture))
            }
            Err(error) => Err(self.record_failure(error)),
        }
    }

    pub async fn list_devices(&self) -> Result<Vec<proto::DeviceInfo>, ConnectError> {
        let sensor = self.sensor.clone();
        let result = match tokio::task::spawn_blocking(move || sensor.list_devices()).await {
            Ok(result) => result,
            Err(join_error) => Err(CaptureError::CaptureFailed(format!("list task failed: {join_error}"))),
        };
        match result {
            Ok(devices) => Ok(devices.into_iter().map(to_proto_device).collect()),
            Err(error) => Err(self.record_failure(error)),
        }
    }

    fn record_success(&self, capture: &Capture) {
        let m = &capture.metadata;
        let tags = [kv("sensor_id", m.sensor_id.as_str()), kv("driver", m.driver)];
        self.log.info(
            "capture done",
            [
                kv("sensor_id", m.sensor_id.as_str()),
                kv("driver", m.driver),
                kv("device", m.device_name.as_str()),
                kv("format", m.format.name()),
                kv("bytes", i64::try_from(m.file_size_bytes).unwrap_or(i64::MAX)),
            ],
        );
        self.metrics.count("captures", 1, tags.clone());
        self.metrics.record("capture_ms", f64::from(m.capture_duration_ms), tags);
    }

    /// Log + count the failure, return the Connect error to send back.
    fn record_failure(&self, error: CaptureError) -> ConnectError {
        let (id, driver) = (self.sensor.id(), self.sensor.driver_name());
        self.log.warn(
            "capture failed",
            [kv("sensor_id", id.as_str()), kv("reason", reason(&error)), kv("error", error.to_string())],
        );
        self.metrics.count("failures", 1, [kv("sensor_id", id), kv("driver", driver), kv("reason", reason(&error))]);
        to_connect_error(error)
    }
}

// ---------------------------------------------------------------------------------------------
// Mapping (pure, unit tested)
// ---------------------------------------------------------------------------------------------

fn to_reply(capture: Capture) -> CaptureReply {
    let m = capture.metadata;
    let metadata = proto::ImageMetadata {
        sensor_id: m.sensor_id,
        kind: m.kind.into(),
        width: m.width,
        height: m.height,
        format: m.format.name().into(),
        mime_type: m.format.mime_type().into(),
        device_name: m.device_name,
        device_index: m.device_index,
        driver: m.driver.into(),
        captured_at_unix_ms: m.captured_at_unix_ms,
        capture_duration_ms: m.capture_duration_ms,
        file_size_bytes: m.file_size_bytes,
        base64_size_bytes: m.base64_size_bytes,
        attributes: m.attributes.into_iter().collect(),
        ..Default::default()
    };
    CaptureReply { image: capture.image, metadata }
}

fn to_proto_device(device: DeviceInfo) -> proto::DeviceInfo {
    proto::DeviceInfo { index: device.index, name: device.name, is_default: device.is_default, ..Default::default() }
}

fn to_connect_error(error: CaptureError) -> ConnectError {
    let code = match &error {
        CaptureError::DeviceNotFound { .. } => ErrorCode::NotFound,
        CaptureError::DriverNotSupported { .. } => ErrorCode::FailedPrecondition,
        CaptureError::DriverUnavailable(_) => ErrorCode::Unavailable,
        CaptureError::DeviceBusy(_) => ErrorCode::ResourceExhausted,
        CaptureError::CaptureFailed(_) | CaptureError::EncodeFailed(_) => ErrorCode::Internal,
    };
    ConnectError::new(code, error.to_string())
}

/// Short, stable label for logs and the `<meter>.failures` metric.
fn reason(error: &CaptureError) -> &'static str {
    match error {
        CaptureError::DeviceNotFound { .. } => "device_not_found",
        CaptureError::DriverNotSupported { .. } => "driver_not_supported",
        CaptureError::DriverUnavailable(_) => "driver_unavailable",
        CaptureError::DeviceBusy(_) => "device_busy",
        CaptureError::CaptureFailed(_) => "capture_failed",
        CaptureError::EncodeFailed(_) => "encode_failed",
    }
}

#[cfg(test)]
#[path = "rpc_tests.rs"]
mod tests;
