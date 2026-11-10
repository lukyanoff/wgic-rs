//! Unit tests for the pure mapping in `handler.rs`.

use crate::domain::{CallFailure, LlmReply};

use super::*;

fn failure(code: &str) -> CallFailure {
    CallFailure { code: code.into(), message: "m".into() }
}

#[test]
fn errors_map_to_connect_codes() {
    let cases = [
        (VisionError::NotConfigured("x".into()), ErrorCode::FailedPrecondition),
        (VisionError::Screen(failure("unavailable")), ErrorCode::Unavailable),
        (VisionError::Screen(failure("deadline_exceeded")), ErrorCode::DeadlineExceeded),
        (VisionError::AllLlmsFailed { summary: "s".into(), all_timed_out: true }, ErrorCode::DeadlineExceeded),
        (VisionError::AllLlmsFailed { summary: "s".into(), all_timed_out: false }, ErrorCode::Unavailable),
    ];
    for (error, code) in cases {
        assert_eq!(to_connect_error(error).code, code);
    }
}

#[test]
fn answers_keep_reply_or_failure() {
    let duration = std::time::Duration::from_millis(5);
    let reply = LlmReply { json: "{}".into(), text: "t".into(), model: "m".into() };
    let ok = answer_message(Answer { caller_id: "a".into(), outcome: Ok(reply), duration });
    assert!(ok.ok && ok.json == "{}" && ok.model == "m" && ok.duration_ms == 5);
    let bad = answer_message(Answer { caller_id: "b".into(), outcome: Err(failure("internal")), duration });
    assert!(!bad.ok && bad.error_code == "internal" && bad.error_message == "m");
}
