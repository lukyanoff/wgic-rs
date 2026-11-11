use super::*;

#[test]
fn known_codes_round_trip_with_their_status() {
    let err = RpcError::from_code("deadline_exceeded", "slow");
    assert_eq!(err, RpcError::deadline_exceeded("slow"));
    assert_eq!(err.status(), 504);
    assert_eq!(RpcError::unavailable("x").status(), 503);
}

#[test]
fn unknown_code_becomes_internal() {
    let err = RpcError::from_code("weird", "boom");
    assert_eq!(err.code, "internal");
    assert_eq!(err.status(), 500);
}

#[test]
fn prefixed_progress_labels_steps_but_not_text() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let progress = ProgressSender::new(tx).prefixed("screen");
    progress.report("capturing");
    progress.text("abc");
    assert_eq!(rx.try_recv().unwrap(), ProgressEvent::Step("[screen] capturing".into()));
    assert_eq!(rx.try_recv().unwrap(), ProgressEvent::Text("abc".into()));
}
