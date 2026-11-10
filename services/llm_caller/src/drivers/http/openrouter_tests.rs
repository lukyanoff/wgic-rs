//! Unit tests for `openrouter.rs`.

use super::*;
use crate::config::DriverKind;
use crate::domain::{ChatMessage, Role};

fn request(schema: Option<&str>) -> LlmRequest {
    let messages = vec![ChatMessage::new(Role::System, "JSON only"), ChatMessage::new(Role::User, "hi")];
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

#[test]
fn image_becomes_an_image_url_part() {
    let mut req = request(None);
    req.messages[1].images.push(crate::domain::Image { data: vec![1, 2, 3], mime_type: "image/jpeg".into() });
    let settings = LlmSettings::new("x", DriverKind::OpenRouter, "m");
    let messages = &OpenRouter.body(&settings, &req)["messages"];
    assert_eq!(messages[0]["content"], json!("JSON only"), "text-only messages stay a string");
    let content = &messages[1]["content"];
    assert_eq!(content[0], json!({"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,AQID"}}));
    assert_eq!(content[1], json!({"type": "text", "text": "hi"}));
}
