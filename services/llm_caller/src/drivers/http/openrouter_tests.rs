//! Unit tests for `openrouter.rs`.

use super::*;
use crate::config::DriverKind;
use crate::domain::{ChatMessage, Role};

fn request(schema: Option<&str>) -> LlmRequest {
    let messages = vec![
        ChatMessage { role: Role::System, content: "JSON only".into() },
        ChatMessage { role: Role::User, content: "hi".into() },
    ];
    LlmRequest { messages, json_schema: schema.map(String::from) }
}

fn event(json: &str) -> SseEvent {
    OpenRouter.parse_event(&serde_json::from_str(json).unwrap())
}

#[test]
fn body_has_messages_effort_schema_and_usage() {
    let mut settings = LlmSettings::new("x", DriverKind::OpenRouter, "vendor/model");
    settings.temperature = Some(1.0);
    let body = OpenRouter.body(&settings, &request(Some(r#"{"type":"array"}"#)));
    assert_eq!(body["model"], "vendor/model");
    assert_eq!(body["messages"][0], json!({ "role": "system", "content": "JSON only" }));
    assert_eq!(body["reasoning"]["effort"], "medium");
    assert_eq!(body["temperature"], 1.0);
    assert_eq!(body["response_format"]["json_schema"]["schema"]["type"], "array");
    assert_eq!(body["usage"]["include"], true);
}

#[test]
fn content_is_text_and_reasoning_is_thinking() {
    assert_eq!(event(r#"{"choices":[{"delta":{"content":"{"}}]}"#), SseEvent::Delta(Delta::Text("{".into())));
    assert_eq!(event(r#"{"choices":[{"delta":{"content":"","reasoning":"hm"}}]}"#), SseEvent::Delta(Delta::Thinking));
}

#[test]
fn usage_chunk_gives_tokens() {
    let parsed = event(r#"{"choices":[{"delta":{}}],"usage":{"prompt_tokens":5,"completion_tokens":7}}"#);
    assert_eq!(parsed, SseEvent::Usage { input_tokens: 5, output_tokens: 7 });
}

#[test]
fn error_chunk_is_failed() {
    let parsed = event(r#"{"error":{"message":"No endpoints found"}}"#);
    assert_eq!(parsed, SseEvent::Failed(LlmError::Failed("openrouter: No endpoints found".into())));
}

#[test]
fn empty_chunks_are_ignored() {
    assert_eq!(event(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#), SseEvent::Ignore);
}
