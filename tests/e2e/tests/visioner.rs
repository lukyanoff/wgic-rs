use contracts::{LlmCallerEndpoint, SensorEndpoint, VisionCaptureRequest, VisionCaptureResponse, VisionerEndpoint};
use e2e::TestEnv;
use e2e::fake_openai::FakeOpenAi;
use llm_caller::{LlmCallerHandler, LlmInstanceState};
use sdk::{MemoryLogger, NatsLlmRegistry, NatsSensorRegistry, ProgressSender};
use sensor::{SensorHandler, SensorInstanceState, SensorKind};
use std::sync::Arc;
use visioner::{Config as VisionerConfig, VisionerClients, VisionerHandler, VisionerInstanceState};

#[tokio::test]
async fn visioner_full_pipeline_with_ha_replicas_and_shared_cas_state() -> anyhow::Result<()> {
    let fake_ai = FakeOpenAi::start().await;
    let mut env = TestEnv::connect().await?;

    let sensor_inst = env.instance_name("cam");
    let llm_inst = env.instance_name("gpt");
    let visioner_inst = env.instance_name("orch");

    let sensor_state = env.state_store::<SensorInstanceState>("sensor", &sensor_inst);
    let sensor_driver = sensor::build_driver(&sensor::Config { kind: SensorKind::Fake, index: 0 });
    env.mount_endpoint(
        "sensor",
        &sensor_inst,
        Arc::new(SensorEndpoint(Arc::new(SensorHandler::new(
            sensor_driver,
            sensor_state,
            Arc::new(MemoryLogger::default()),
        )))),
    )
    .await?;

    let llm_state = env.state_store::<LlmInstanceState>("llm_caller", &llm_inst);
    let llm_driver = llm_caller::build_driver(llm_caller::Config {
        kind: llm_caller::DriverKind::Http,
        model: "ha-vision-model".into(),
        endpoint: fake_ai.url(),
        api_key: "secret".into(),
        ..Default::default()
    })?;
    env.mount_endpoint(
        "llm_caller",
        &llm_inst,
        Arc::new(LlmCallerEndpoint(Arc::new(LlmCallerHandler::new(
            llm_driver,
            llm_state,
            Arc::new(MemoryLogger::default()),
        )))),
    )
    .await?;

    let shared_visioner_state = env.state_store::<VisionerInstanceState>("visioner", &visioner_inst);
    let v_cfg = VisionerConfig {
        sensor: sensor_inst.clone(),
        llm_caller: llm_inst.clone(),
        system_prompt: "System".into(),
        text_prompt: "Text".into(),
        user_prompt: "Describe desk".into(),
        ..Default::default()
    };

    for _replica in 0..2 {
        let handler = Arc::new(VisionerHandler::new(
            v_cfg.clone(),
            shared_visioner_state.clone(),
            VisionerClients {
                sensors: Arc::new(NatsSensorRegistry::new(env.nc.clone(), e2e::CALL_TIMEOUT)),
                llms: Arc::new(NatsLlmRegistry::new(env.nc.clone(), e2e::CALL_TIMEOUT)),
            },
            Arc::new(MemoryLogger::default()),
        ));
        env.mount_endpoint("visioner", &visioner_inst, Arc::new(VisionerEndpoint(handler))).await?;
    }

    let client = env.visioner_client(&visioner_inst);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            c.call::<VisionCaptureRequest, VisionCaptureResponse>(
                "visioner.v1.VisionerService",
                "Capture",
                &VisionCaptureRequest::default(),
                &ProgressSender::noop(),
            )
            .await
        }));
    }

    for t in tasks {
        let res = t.await??;
        assert_eq!(res.model, "ha-vision-model");
        assert_eq!(res.sensor_mime_type, "image/png");
    }

    let final_state = shared_visioner_state.get().await?;
    assert_eq!(final_state.total_captures, 4);
    assert_eq!(final_state.last_model_used, "ha-vision-model");

    Ok(())
}
