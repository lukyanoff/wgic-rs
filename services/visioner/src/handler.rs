use crate::config::Config;
use async_trait::async_trait;
use contracts::{
    ILlmCallerService, ISensorService, IVisionerService, ImageInput, LlmCallRequest, SensorCaptureRequest,
    VisionCaptureRequest, VisionCaptureResponse,
};
use sdk::state::IStateStore;
use sdk::{ClientRegistry, ILogger, ProgressSender, RpcError, json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisionerInstanceState {
    pub total_captures: u64,
    pub last_model_used: String,
}

/// The downstream services the visioner calls, by instance name.
pub struct VisionerClients {
    pub sensors: Arc<dyn ClientRegistry<dyn ISensorService>>,
    pub llms: Arc<dyn ClientRegistry<dyn ILlmCallerService>>,
}

pub struct VisionerHandler {
    cfg: Config,
    state: Arc<dyn IStateStore<VisionerInstanceState>>,
    clients: VisionerClients,
    log: Arc<dyn ILogger>,
}

impl VisionerHandler {
    /// Pure Constructor Dependency Injection (SOLID)
    pub fn new(
        cfg: Config,
        state: Arc<dyn IStateStore<VisionerInstanceState>>,
        clients: VisionerClients,
        log: Arc<dyn ILogger>,
    ) -> Self {
        Self { cfg, state, clients, log }
    }

    /// Log a failed pipeline step and pass the error on unchanged.
    fn failed(&self, step: &str, e: RpcError) -> RpcError {
        self.log.warn("step failed", json!({ "step": step, "error.type": e.code, "error.message": e.message }));
        e
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

        let sensor = self.clients.sensors.get(sensor_target)?;
        let llm = self.clients.llms.get(llm_target)?;

        // Each source's steps arrive labelled, e.g. `[screen] capturing from screen`, `[agy] thinking`.
        progress.report(format!("capturing from {sensor_target}"));
        let snap = sensor
            .capture(SensorCaptureRequest {}, progress.prefixed(sensor_target))
            .await
            .map_err(|e| self.failed("capture", e))?;
        if snap.data.is_empty() {
            return Err(RpcError::internal("sensor returned empty image"));
        }

        progress.report(format!("asking {llm_target} ({} KB {})", snap.data.len() / 1024, snap.mime_type));
        let request = LlmCallRequest {
            system_prompt: pick(req.system_prompt.as_deref(), &self.cfg.system_prompt).to_string(),
            text_prompt: pick(req.text_prompt.as_deref(), &self.cfg.text_prompt).to_string(),
            user_prompt: user_prompt.to_string(),
            images: vec![ImageInput { data: snap.data, mime_type: snap.mime_type.clone() }],
        };
        let llm_reply = llm.call(request, progress.prefixed(llm_target)).await.map_err(|e| self.failed("ask", e))?;

        let model_used = llm_reply.model.clone();
        self.state
            .update_cas(Box::new(move |s| {
                s.total_captures += 1;
                s.last_model_used = model_used.clone();
            }))
            .await
            .map_err(|e| RpcError::internal(format!("state CAS failed: {e}")))?;

        progress.report("done");
        let fields = json!({ "sensor": sensor_target, "llm_caller": llm_target, "model": llm_reply.model });
        self.log.info("pipeline completed", fields);

        Ok(VisionCaptureResponse { text: llm_reply.text, model: llm_reply.model, sensor_mime_type: snap.mime_type })
    }
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod handler_tests;
