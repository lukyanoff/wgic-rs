use super::*;
use contracts::{LlmCallResponse, SensorCaptureResponse};
use sdk::state::InMemoryStateStore;
use sdk::MapRegistry;
use std::collections::HashMap;

struct FakeSensor;
#[async_trait]
impl ISensorService for FakeSensor {
    async fn capture(
        &self,
        _req: SensorCaptureRequest,
        progress: ProgressSender,
    ) -> Result<SensorCaptureResponse, RpcError> {
        progress.report("sensor click");
        Ok(SensorCaptureResponse {
            data: vec![0x89, 0x50, 0x4E, 0x47],
            mime_type: "image/png".into(),
        })
    }
}

struct FakeLlm;
#[async_trait]
impl ILlmCallerService for FakeLlm {
    async fn call(
        &self,
        req: LlmCallRequest,
        progress: ProgressSender,
    ) -> Result<LlmCallResponse, RpcError> {
        progress.report("llm thinking");
        assert_eq!(req.images.len(), 1);
        Ok(LlmCallResponse {
            text: format!("described: {}", req.user_prompt),
            model: "fake-gpt".into(),
        })
    }
}

#[tokio::test]
async fn capture_runs_pipeline_and_updates_cas_state() {
    let state = Arc::new(InMemoryStateStore::new(VisionerInstanceState::default()));

    let mut sensors_map: HashMap<String, Arc<dyn ISensorService>> = HashMap::new();
    sensors_map.insert("cam1".into(), Arc::new(FakeSensor));

    let mut llms_map: HashMap<String, Arc<dyn ILlmCallerService>> = HashMap::new();
    llms_map.insert("agy".into(), Arc::new(FakeLlm));

    let handler = VisionerHandler::new(
        Config {
            sensor: "cam1".into(),
            llm_caller: "agy".into(),
            user_prompt: "what is on my desk?".into(),
            ..Default::default()
        },
        state.clone(),
        Arc::new(MapRegistry::new(sensors_map)),
        Arc::new(MapRegistry::new(llms_map)),
    );

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let resp = handler
        .capture(VisionCaptureRequest::default(), ProgressSender::new(tx))
        .await
        .unwrap();

    assert_eq!(resp.text, "described: what is on my desk?");
    assert_eq!(resp.model, "fake-gpt");
    assert_eq!(resp.sensor_mime_type, "image/png");

    let updated_state = state.get().await.unwrap();
    assert_eq!(updated_state.total_captures, 1);
    assert_eq!(updated_state.last_model_used, "fake-gpt");

    let mut steps = Vec::new();
    while let Ok(step) = rx.try_recv() {
        steps.push(step);
    }
    assert_eq!(
        steps,
        vec![
            "capturing image from cam1",
            "sensor click",
            "calling LLM agy",
            "llm thinking",
            "done"
        ]
    );
}
