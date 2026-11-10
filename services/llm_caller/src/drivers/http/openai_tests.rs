//! Unit tests for `openai.rs`.

use super::*;
use crate::config::DriverKind;
use crate::domain::{ChatMessage, Role};

fn request(schema: Option<&str>) -> LlmRequest {
    let messages = vec![ChatMessage { role: Role::User, content: "hi".into() }];
    LlmRequest { messages, json_schema: schema.map(String::from) }
}

fn event(json: &str) -> SseEvent {
    OpenAi.parse_event(&serde_json::from_str(json).unwrap())
}

#[test]
fn body_has_model_effort_temperature_and_schema() {
    let mut settings = LlmSettings::new("x", DriverKind::OpenAi, "gpt-x");
    settings.effort = "low".into();
    settings.temperature = Some(0.3);
    let body = OpenAi.body(&settings, &request(Some(r#"{"type":"object"}"#)));
    assert_eq!(body["model"], "gpt-x");
    assert_eq!(body["stream"], true);
    assert_eq!(body["reasoning"]["effort"], "low");
    assert_eq!(body["temperature"], 0.3);
    assert_eq!(body["input"][0], json!({ "role": "user", "content": "hi" }));
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert_eq!(body["text"]["format"]["schema"]["type"], "object");
}

#[test]
fn without_schema_any_json_object_is_asked_and_no_temperature_is_sent() {
    let body = OpenAi.body(&LlmSettings::new("x", DriverKind::OpenAi, "m"), &request(None));
    assert_eq!(body["text"]["format"], json!({ "type": "json_object" }));
    assert!(body.get("temperature").is_none());
}

#[test]
fn text_delta_is_text() {
    let parsed = event(r#"{"type":"response.output_text.delta","delta":"{\"a\""}"#);
    assert_eq!(parsed, SseEvent::Delta(Delta::Text("{\"a\"".into())));
}

#[test]
fn reasoning_delta_is_thinking() {
    assert_eq!(
        event(r#"{"type":"response.reasoning_summary_text.delta","delta":"hm"}"#),
        SseEvent::Delta(Delta::Thinking)
    );
}

#[test]
fn completed_gives_usage() {
    let parsed = event(r#"{"type":"response.completed","response":{"usage":{"input_tokens":12,"output_tokens":34}}}"#);
    assert_eq!(parsed, SseEvent::Usage { input_tokens: 12, output_tokens: 34 });
}

#[test]
fn failed_response_is_failed_with_message() {
    let parsed = event(r#"{"type":"response.failed","response":{"error":{"message":"server overloaded"}}}"#);
    assert_eq!(parsed, SseEvent::Failed(LlmError::Failed("openai: server overloaded".into())));
}
