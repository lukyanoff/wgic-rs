use crate::drivers::{CaptureOutput, Driver};
use async_trait::async_trait;
use image::ImageFormat;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use sdk::RpcError;
use std::io::Cursor;

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
            cam.open_stream()
                .map_err(|e| RpcError::internal(format!("camera open_stream failed: {e}")))?;
            let frame = cam
                .frame()
                .map_err(|e| RpcError::internal(format!("camera frame failed: {e}")))?;
            let decoded = frame
                .decode_image::<RgbFormat>()
                .map_err(|e| RpcError::internal(format!("camera decode failed: {e}")))?;

            let mut buf = Vec::new();
            decoded
                .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
                .map_err(|e| RpcError::internal(format!("png encode failed: {e}")))?;

            Ok(CaptureOutput {
                data: buf,
                mime_type: "image/png".to_string(),
            })
        })
        .await
        .map_err(|e| RpcError::internal(format!("camera task panicked: {e}")))?
    }
}
