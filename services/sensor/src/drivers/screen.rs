use crate::drivers::{CaptureOutput, Driver};
use async_trait::async_trait;
use image::ImageFormat;
use sdk::RpcError;
use std::io::Cursor;

pub struct ScreenDriver {
    index: u32,
}

impl ScreenDriver {
    pub fn new(index: u32) -> Self {
        Self { index }
    }
}

#[async_trait]
impl Driver for ScreenDriver {
    fn kind_name(&self) -> &'static str {
        "screen"
    }

    async fn capture(&self) -> Result<CaptureOutput, RpcError> {
        let idx = self.index as usize;
        tokio::task::spawn_blocking(move || {
            let monitors = xcap::Monitor::all()
                .map_err(|e| RpcError::internal(format!("xcap monitor list failed: {e}")))?;
            let monitor = monitors.get(idx).ok_or_else(|| {
                RpcError::not_found(format!(
                    "screen index {idx} not found (available: {})",
                    monitors.len()
                ))
            })?;
            let frame = monitor
                .capture_image()
                .map_err(|e| RpcError::internal(format!("screen capture failed: {e}")))?;

            let mut buf = Vec::new();
            frame
                .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
                .map_err(|e| RpcError::internal(format!("png encode failed: {e}")))?;

            Ok(CaptureOutput {
                data: buf,
                mime_type: "image/png".to_string(),
            })
        })
        .await
        .map_err(|e| RpcError::internal(format!("screen capture task panicked: {e}")))?
    }
}
