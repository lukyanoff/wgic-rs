use crate::drivers::{CaptureOutput, Driver};
use async_trait::async_trait;
use image::{ImageBuffer, ImageFormat, Rgba};
use sdk::RpcError;
use std::io::Cursor;

pub struct FakeDriver;

#[async_trait]
impl Driver for FakeDriver {
    fn kind_name(&self) -> &'static str {
        "fake"
    }

    async fn capture(&self) -> Result<CaptureOutput, RpcError> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(32, 32, |x, y| {
            Rgba([(x * 8) as u8, (y * 8) as u8, 180, 255])
        });
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .map_err(|e| RpcError::internal(format!("fake png encode error: {e}")))?;

        Ok(CaptureOutput {
            data: buf,
            mime_type: "image/png".to_string(),
        })
    }
}
