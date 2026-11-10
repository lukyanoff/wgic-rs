//! Unit tests for `claude.rs`, with lines recorded from a real `claude -p` run.

use super::*;
use crate::drivers::{ChatMessage, Image};

fn request(schema: &str) -> CallRequest {
    let message =
        |role: &str, content: &str| ChatMessage { role: role.into(), content: content.into(), ..Default::default() };
    let messages = vec![message("system", "Answer in JSON."), message("user", "hi")];
    CallRequest { messages, json_schema: schema.into(), ..Default::default() }
}

fn reply(line: &str) -> LlmReply {
    let LineEvent::Done(Ok(reply)) = ClaudeCli.parse_line(line) else { panic!("expected a reply") };
    reply
}

#[test]
fn args_carry_model_effort_system_prompt_and_schema() {
    let settings = LlmSettings { model: "claude-haiku-5-5".into(), effort: "low".into(), ..Default::default() };
    let args = ClaudeCli.args(&settings, &request("{}")).join(" ");
    assert!(args.starts_with("-p --input-format stream-json --output-format stream-json"));
    assert!(args.contains("--model claude-haiku-5-5 --effort low --system-prompt Answer in JSON."));
    assert!(args.ends_with("--json-schema {}"));
}

#[test]
fn images_go_to_stdin_first_as_base64_blocks() {
    let mut req = request("");
    req.messages[1].images.push(Image { data: vec![1, 2, 3], mime_type: "image/png".into(), ..Default::default() });
    let value: Value = serde_json::from_str(&ClaudeCli.stdin(&req).expect("stdin")).expect("one JSON line");
    let content = &value["message"]["content"];
    assert_eq!(content[0]["source"], json!({"type": "base64", "media_type": "image/png", "data": "AQID"}));
    assert_eq!(content[1], json!({"type": "text", "text": "hi"}));
}

#[test]
fn deltas_are_text_or_thinking() {
    let text = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"{\"items\""}}}"#;
    let thinking = r#"{"type":"stream_event","event":{"delta":{"type":"thinking_delta","thinking":"hm"}}}"#;
    assert!(matches!(ClaudeCli.parse_line(text), LineEvent::Delta(Delta::Text(t)) if t == "{\"items\""));
    assert!(matches!(ClaudeCli.parse_line(thinking), LineEvent::Delta(Delta::Thinking)));
    assert!(matches!(ClaudeCli.parse_line(r#"{"type":"system","subtype":"init"}"#), LineEvent::Ignore));
}

#[test]
fn result_line_gives_text_and_tokens() {
    let line = r#"{"type":"result","is_error":false,"result":"{\"a\":1}","usage":{"input_tokens":2,"cache_creation_input_tokens":100,"cache_read_input_tokens":10,"output_tokens":219}}"#;
    assert_eq!(reply(line), LlmReply { text: "{\"a\":1}".into(), input_tokens: 112, output_tokens: 219 });
}

#[test]
fn structured_output_wins_over_result_text() {
    let line = r#"{"type":"result","is_error":false,"result":"Paris it is","structured_output":{"city":"Paris"}}"#;
    assert_eq!(reply(line).text, r#"{"city":"Paris"}"#);
}

#[test]
fn error_result_is_internal() {
    let line = r#"{"type":"result","is_error":true,"result":"model not found"}"#;
    let LineEvent::Done(Err(error)) = ClaudeCli.parse_line(line) else { panic!("expected an error") };
    assert_eq!(error.message.as_deref(), Some("claude: model not found"));
}
