//! Unit tests for `clients.rs` (the RPC paths are covered by `tests/rpc.rs`).

use super::*;

#[test]
fn request_has_question_screenshot_and_schema() {
    let info = ScreenInfo { mime_type: "image/jpeg".into(), ..Default::default() };
    let request = call_request("what?", &Screenshot { image: vec![1, 2], info });
    let message = &request.messages[0];
    assert_eq!((message.role.as_str(), message.content.as_str()), ("user", "what?"));
    assert_eq!(
        (message.images[0].data.as_slice(), message.images[0].mime_type.as_str()),
        (&[1u8, 2][..], "image/jpeg")
    );
    assert_eq!(request.json_schema, ANSWER_SCHEMA);
}

#[test]
fn connect_error_becomes_its_code_name() {
    let error = ConnectError::new(connectrpc::ErrorCode::DeadlineExceeded, "too slow");
    assert_eq!(failure(error), CallFailure { code: "deadline_exceeded".into(), message: "too slow".into() });
}
