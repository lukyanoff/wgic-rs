use super::*;

#[test]
fn frames_round_trip() {
    for frame in [StreamFrame::Progress("step".into()), StreamFrame::Text("abc".into()), StreamFrame::Ok(vec![1, 2])] {
        assert_eq!(StreamFrame::decode(&frame.encode()).unwrap(), frame);
    }
}

#[test]
fn service_error_text_keeps_its_code() {
    assert_eq!(parse_service_error("deadline_exceeded: no answer"), RpcError::deadline_exceeded("no answer"));
    assert_eq!(parse_service_error("plain text"), RpcError::internal("plain text"));
}
