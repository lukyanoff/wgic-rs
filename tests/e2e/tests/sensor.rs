use contracts::{ISensorService, SensorCaptureRequest, SensorEndpoint};
use e2e::TestEnv;
use sdk::ProgressSender;
use sensor::{build_driver, Config, SensorHandler, SensorInstanceState, SensorKind};
use std::sync::Arc;

#[tokio::test]
async fn sensor_captures_frame_streams_progress_and_updates_kv_state() -> anyhow::Result<()> {
    let mut env = TestEnv::connect().await?;
    let inst = env.instance_name("fake_cam");

    let cfg = Config { kind: SensorKind::Fake, index: 0 };
    let state = env.state_store::<SensorInstanceState>("sensor", &inst);
    let handler = Arc::new(SensorHandler::new(build_driver(&cfg), state.clone()));

    env.mount_endpoint("sensor", &inst, Arc::new(SensorEndpoint(handler))).await?;

    let client = env.sensor_client(&inst);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let res = client.capture(SensorCaptureRequest {}, ProgressSender::new(tx)).await?;

    assert!(!res.data.is_empty());
    assert_eq!(res.mime_type, "image/png");

    let mut progress_steps = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        progress_steps.push(msg);
    }
    assert!(progress_steps.iter().any(|s| s.contains("capturing from")));

    let saved = state.get().await?;
    assert_eq!(saved.frames_captured, 1);
    assert_eq!(saved.last_mime_type, "image/png");
    Ok(())
}
