use crate::drivers::Driver;
use async_trait::async_trait;
use contracts::{ISensorService, SensorCaptureRequest, SensorCaptureResponse};
use sdk::state::IStateStore;
use sdk::{ILogger, ProgressSender, RpcError, json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensorInstanceState {
    pub frames_captured: u64,
    pub last_mime_type: String,
    pub last_bytes_size: usize,
}

pub struct SensorHandler {
    driver: Arc<dyn Driver>,
    state: Arc<dyn IStateStore<SensorInstanceState>>,
    log: Arc<dyn ILogger>,
}

impl SensorHandler {
    pub fn new(
        driver: Arc<dyn Driver>,
        state: Arc<dyn IStateStore<SensorInstanceState>>,
        log: Arc<dyn ILogger>,
    ) -> Self {
        Self { driver, state, log }
    }
}

#[async_trait]
impl ISensorService for SensorHandler {
    #[tracing::instrument(skip(self, _req, progress), fields(driver = %self.driver.kind_name()))]
    async fn capture(
        &self,
        _req: SensorCaptureRequest,
        progress: ProgressSender,
    ) -> Result<SensorCaptureResponse, RpcError> {
        progress.report(format!("capturing from {}", self.driver.kind_name()));

        let out = self.driver.capture().await?;

        let mime = out.mime_type.clone();
        let size = out.data.len();
        self.state
            .update_cas(Box::new(move |s| {
                s.frames_captured += 1;
                s.last_mime_type = mime.clone();
                s.last_bytes_size = size;
            }))
            .await
            .map_err(|e| RpcError::internal(format!("sensor state CAS failed: {e}")))?;

        progress.report(format!("captured {size} bytes"));
        self.log.info("frame captured", json!({ "bytes": size, "mime_type": out.mime_type }));

        Ok(SensorCaptureResponse { data: out.data, mime_type: out.mime_type })
    }
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod handler_tests;
