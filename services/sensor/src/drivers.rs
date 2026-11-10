pub mod camera;
pub mod fake;
pub mod screen;

use crate::config::{Config, SensorKind};
use async_trait::async_trait;
use sdk::RpcError;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CaptureOutput {
    pub data: Vec<u8>,
    pub mime_type: String,
}

#[async_trait]
pub trait Driver: Send + Sync + 'static {
    fn kind_name(&self) -> &'static str;
    async fn capture(&self) -> Result<CaptureOutput, RpcError>;
}

// ============================================================================
// Shared frame encoding (screen + camera)
// ============================================================================

/// Longest edge of a frame we send on. Vision LLMs downscale bigger images anyway (Claude to ~1568 px),
/// and a NATS message is capped at 1 MB, which a 4K PNG screenshot already exceeds.
pub const MAX_EDGE: u32 = 1568;
const JPEG_QUALITY: u8 = 85;

/// Downscales to `MAX_EDGE` (keeping aspect ratio) and encodes as JPEG: a 4K screen becomes ~200-400 KB.
pub fn encode_frame(img: image::DynamicImage) -> Result<CaptureOutput, RpcError> {
    let img = if img.width().max(img.height()) > MAX_EDGE {
        img.resize(MAX_EDGE, MAX_EDGE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut data = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, JPEG_QUALITY)
        .encode_image(&img.to_rgb8()) // JPEG has no alpha channel
        .map_err(|e| RpcError::internal(format!("jpeg encode failed: {e}")))?;
    Ok(CaptureOutput { data, mime_type: "image/jpeg".to_string() })
}

pub fn build_driver(cfg: &Config) -> Arc<dyn Driver> {
    match cfg.kind {
        SensorKind::Fake => Arc::new(fake::FakeDriver),
        SensorKind::Screen => Arc::new(screen::ScreenDriver::new(cfg.index)),
        SensorKind::Camera => Arc::new(camera::CameraDriver::new(cfg.index)),
    }
}
