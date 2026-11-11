use super::*;
use crate::drivers::fake::FakeDriver;
use sdk::state::InMemoryStateStore;

#[tokio::test]
async fn sensor_handler_captures_fake_frame_and_updates_cas_state() {
    let state = Arc::new(InMemoryStateStore::new(SensorInstanceState::default()));
    let handler = SensorHandler::new(Arc::new(FakeDriver), state.clone());

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let res = handler
        .capture(SensorCaptureRequest {}, ProgressSender::new(tx))
        .await
        .unwrap();

    assert_eq!(res.mime_type, "image/png");
    assert!(!res.data.is_empty());

    let updated = state.get().await.unwrap();
    assert_eq!(updated.frames_captured, 1);
    assert_eq!(updated.last_mime_type, "image/png");
    assert_eq!(updated.last_bytes_size, res.data.len());

    let mut steps = Vec::new();
    while let Ok(s) = rx.try_recv() {
        steps.push(s);
    }
    assert_eq!(steps.len(), 2);
}
