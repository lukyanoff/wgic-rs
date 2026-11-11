use crate::drivers::{CaptureOutput, Driver, encode_frame};
use async_trait::async_trait;
use nokhwa::Camera;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use sdk::RpcError;

/// Frames read per capture; only the last is kept (see `capture`).
const WARMUP_FRAMES: usize = 5;

pub struct CameraDriver {
    index: u32,
}

impl CameraDriver {
    pub fn new(index: u32) -> Self {
        Self { index }
    }
}

#[async_trait]
impl Driver for CameraDriver {
    fn kind_name(&self) -> &'static str {
        "camera"
    }

    async fn capture(&self) -> Result<CaptureOutput, RpcError> {
        let idx = self.index;
        tokio::task::spawn_blocking(move || {
            let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            let mut cam = Camera::new(CameraIndex::Index(idx), format)
                .map_err(|e| RpcError::internal(format!("camera init failed: {e}")))?;
            cam.open_stream().map_err(|e| RpcError::internal(format!("camera open_stream failed: {e}")))?;
            // The first frames after opening are often dark while auto-exposure settles; keep the last one.
            let mut frame = None;
            for _ in 0..WARMUP_FRAMES {
                frame = Some(cam.frame().map_err(|e| RpcError::internal(format!("camera frame failed: {e}")))?);
            }
            let decoded = frame
                .expect("WARMUP_FRAMES > 0")
                .decode_image::<RgbFormat>()
                .map_err(|e| RpcError::internal(format!("camera decode failed: {e}")))?;
            encode_frame(image::DynamicImage::ImageRgb8(decoded))
        })
        .await
        .map_err(|e| RpcError::internal(format!("camera task panicked: {e}")))?
    }
}
