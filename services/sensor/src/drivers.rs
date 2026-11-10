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

pub fn build_driver(cfg: &Config) -> Arc<dyn Driver> {
    match cfg.kind {
        SensorKind::Fake => Arc::new(fake::FakeDriver),
        SensorKind::Screen => Arc::new(screen::ScreenDriver::new(cfg.index)),
        SensorKind::Camera => Arc::new(camera::CameraDriver::new(cfg.index)),
    }
}
