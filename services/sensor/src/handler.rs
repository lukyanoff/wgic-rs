//! `sensor.v1.SensorService`: grab a frame with the configured driver, encode it as PNG/JPEG and describe it.
//! Failed calls need no code here: the platform logs and measures every RPC with its error code.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::io::Cursor;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceResult};
use contracts::proto::sensor::v1::{
    CaptureRequest, CaptureResponse, ImageMetadata, ListDevicesRequest, ListDevicesResponse, SensorService,
};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, RgbaImage};

use crate::config::{ImageFormat, SensorSettings};
use crate::drivers::{FrameSource, RawFrame};

// ---------------------------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------------------------

/// ≈ `class SensorHandler : SensorService.SensorServiceBase`. `Arc<dyn FrameSource>` ≈ an `IFrameSource` field.
pub struct SensorHandler {
    pub settings: SensorSettings,
    pub source: Arc<dyn FrameSource>,
}

#[allow(refining_impl_trait)]
impl SensorService for SensorHandler {
    async fn capture(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, CaptureRequest>,
    ) -> ServiceResult<CaptureResponse> {
        let captured_at = unix_ms(SystemTime::now());
        let started = Instant::now();
        let (source, device) = (self.source.clone(), self.settings.device);
        let frame = run_blocking(move || source.grab(device)).await?;
        let capture_ms = started.elapsed().as_millis() as u32;

        let image = encode(&frame, self.settings.format)?;
        let metadata = ImageMetadata {
            sensor_id: self.settings.id.clone(),
            kind: self.source.kind().into(),
            width: frame.width,
            height: frame.height,
            mime_type: self.settings.format.mime_type().into(),
            device_name: frame.device.name,
            device_index: frame.device.index,
            captured_at_unix_ms: captured_at,
            capture_ms,
            ..Default::default()
        };
        Response::ok(CaptureResponse { image, metadata: metadata.into(), ..Default::default() })
    }

    async fn list_devices(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, ListDevicesRequest>,
    ) -> ServiceResult<ListDevicesResponse> {
        let source = self.source.clone();
        let devices = run_blocking(move || source.list_devices()).await?;
        Response::ok(ListDevicesResponse { devices, ..Default::default() })
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// Run blocking OS work on a worker thread, so it doesn't stall other requests (≈ `await Task.Run(...)`).
async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ConnectError> + Send + 'static,
) -> Result<T, ConnectError> {
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(error) => Err(ConnectError::internal(format!("capture thread failed: {error}"))),
    }
}

/// RGBA pixels → PNG or JPEG file bytes. JPEG has no alpha channel, so it is dropped first.
fn encode(frame: &RawFrame, format: ImageFormat) -> Result<Vec<u8>, ConnectError> {
    let Some(pixels) = RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone()) else {
        let size = format!("{} bytes for a {}x{} frame", frame.rgba.len(), frame.width, frame.height);
        return Err(ConnectError::internal(format!("driver returned {size}")));
    };
    let mut bytes = Vec::new();
    let result = match format {
        ImageFormat::Png => pixels.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png),
        ImageFormat::Jpeg => {
            let rgb = DynamicImage::ImageRgba8(pixels).to_rgb8();
            JpegEncoder::new_with_quality(&mut bytes, 85).encode_image(&rgb)
        }
    };
    match result {
        Ok(()) => Ok(bytes),
        Err(error) => Err(ConnectError::internal(format!("image encoding failed: {error}"))),
    }
}

fn unix_ms(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(since_epoch) => since_epoch.as_millis() as i64,
        Err(_) => 0,
    }
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
