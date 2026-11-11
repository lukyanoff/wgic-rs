use crate::config::Config;
use async_trait::async_trait;
use contracts::{
    ILlmCallerService, ISensorService, IVisionerService, ImageInput, LlmCallRequest,
    SensorCaptureRequest, VisionCaptureRequest, VisionCaptureResponse,
};
use sdk::state::IStateStore;
use sdk::{ClientRegistry, ProgressSender, RpcError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisionerInstanceState {
    pub total_captures: u64,
    pub last_model_used: String,
}

pub struct VisionerHandler {
    cfg: Config,
    state: Arc<dyn IStateStore<VisionerInstanceState>>,
    sensors: Arc<dyn ClientRegistry<dyn ISensorService>>,
    llms: Arc<dyn ClientRegistry<dyn ILlmCallerService>>,
}

impl VisionerHandler {
    /// Pure Constructor Dependency Injection (SOLID)
    pub fn new(
        cfg: Config,
        state: Arc<dyn IStateStore<VisionerInstanceState>>,
        sensors: Arc<dyn ClientRegistry<dyn ISensorService>>,
        llms: Arc<dyn ClientRegistry<dyn ILlmCallerService>>,
    ) -> Self {
        Self { cfg, state, sensors, llms }
    }
}

fn pick<'a>(override_val: Option<&'a str>, fallback: &'a str) -> &'a str {
    match override_val {
        Some(s) if !s.is_empty() => s,
        _ => fallback,
    }
}

#[async_trait]
impl IVisionerService for VisionerHandler {
    #[tracing::instrument(skip(self, req, progress))]
    async fn capture(
        &self,
        req: VisionCaptureRequest,
        progress: ProgressSender,
    ) -> Result<VisionCaptureResponse, RpcError> {
        let sensor_target = pick(req.sensor.as_deref(), &self.cfg.sensor);
        let llm_target = pick(req.llm_caller.as_deref(), &self.cfg.llm_caller);
        let user_prompt = pick(req.user_prompt.as_deref(), &self.cfg.user_prompt);

        if sensor_target.is_empty() {
            return Err(RpcError::invalid_arg("sensor target is required"));
        }
        if llm_target.is_empty() {
            return Err(RpcError::invalid_arg("llm_caller target is required"));
        }
        if user_prompt.is_empty() {
            return Err(RpcError::invalid_arg("user_prompt is required"));
        }

        let sensor = self.sensors.get(sensor_target)?;
        let llm = self.llms.get(llm_target)?;

        progress.report(format!("capturing image from {sensor_target}"));
        let snap = sensor
            .capture(SensorCaptureRequest {}, progress.clone())
            .await?;

        if snap.data.is_empty() {
            return Err(RpcError::internal("sensor returned empty image"));
        }

        progress.report(format!("calling LLM {llm_target}"));
        let llm_reply = llm
            .call(
                LlmCallRequest {
                    system_prompt: pick(req.system_prompt.as_deref(), &self.cfg.system_prompt).to_string(),
                    text_prompt: pick(req.text_prompt.as_deref(), &self.cfg.text_prompt).to_string(),
                    user_prompt: user_prompt.to_string(),
                    images: vec![ImageInput {
                        data: snap.data,
                        mime_type: snap.mime_type.clone(),
                    }],
                },
                progress.clone(),
            )
            .await?;

        let model_used = llm_reply.model.clone();
        self.state
            .update_cas(Box::new(move |s| {
                s.total_captures += 1;
                s.last_model_used = model_used.clone();
            }))
            .await
            .map_err(|e| RpcError::internal(format!("state CAS failed: {e}")))?;

        progress.report("done");
        tracing::info!(
            sensor = %sensor_target,
            llm = %llm_target,
            model = %llm_reply.model,
            "visioner pipeline completed"
        );

        Ok(VisionCaptureResponse {
            text: llm_reply.text,
            model: llm_reply.model,
            sensor_mime_type: snap.mime_type,
        })
    }
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod handler_tests;
