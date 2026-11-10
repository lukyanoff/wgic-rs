//! Unit tests for the pure rules in `handler.rs`.

use super::*;

fn failed(id: &str, code: ErrorCode) -> LlmAnswer {
    LlmAnswer {
        caller_id: id.into(),
        error_code: code.as_str().into(),
        error_message: "x".into(),
        ..Default::default()
    }
}

#[test]
fn one_answer_is_enough() {
    let ok = LlmAnswer { ok: true, ..Default::default() };
    assert!(all_failed(&[failed("a", ErrorCode::Internal), ok]).is_none());
}

#[test]
fn all_failed_lists_each_failure() {
    let error = all_failed(&[failed("a", ErrorCode::Internal), failed("b", ErrorCode::DeadlineExceeded)]).unwrap();
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert_eq!(error.message.as_deref(), Some("every model failed: a: x (internal); b: x (deadline_exceeded)"));
}

#[test]
fn all_timed_out_is_deadline_exceeded() {
    let error = all_failed(&[failed("a", ErrorCode::DeadlineExceeded)]).unwrap();
    assert_eq!(error.code, ErrorCode::DeadlineExceeded);
}

#[test]
fn screen_errors_become_unavailable_unless_timed_out() {
    assert_eq!(no_screenshot(&ConnectError::not_found("no display")).code, ErrorCode::Unavailable);
    assert_eq!(no_screenshot(&ConnectError::deadline_exceeded("slow")).code, ErrorCode::DeadlineExceeded);
}
