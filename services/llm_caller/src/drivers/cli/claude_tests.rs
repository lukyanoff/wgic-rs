//! Unit tests for `claude.rs`, with lines recorded from a real `claude -p` run.

use super::*;
use crate::config::DriverKind;
use crate::domain::{ChatMessage, Image, Role};

fn request(schema: Option<&str>) -> LlmRequest {
    let messages = vec![ChatMessage::new(Role::System, "Answer in JSON."), ChatMessage::new(Role::User, "hi")];
    LlmRequest { messages, json_schema: schema.map(String::from) }
}

#[test]
fn args_carry_model_effort_system_prompt_and_schema() {
    let mut settings = LlmSettings::new("x", DriverKind::ClaudeCli, "claude-haiku-5-5");
    settings.effort = "low".into();
    let args = ClaudeCli.args(&settings, &request(Some("{}"))).join(" ");
    assert!(args.starts_with("-p --input-format stream-json --output-format stream-json"));
    assert!(args.contains("--model claude-haiku-5-5 --effort low"));
    assert!(args.contains("--system-prompt Answer in JSON."));
    assert!(args.ends_with("--json-schema {}"));
}

#[test]
fn prompt_goes_to_stdin_as_one_user_line() {
    let line = ClaudeCli.stdin(&request(None)).expect("stdin");
    let value: Value = serde_json::from_str(&line).expect("one JSON line");
    assert_eq!(value["type"], "user");
    assert_eq!(value["message"]["content"], serde_json::json!([{"type": "text", "text": "hi"}]));
}

#[test]
fn images_go_first_as_base64_blocks() {
    let mut req = request(None);
    req.messages[1].images.push(Image { data: vec![1, 2, 3], mime_type: "image/png".into() });
    let value: Value = serde_json::from_str(&ClaudeCli.stdin(&req).expect("stdin")).expect("JSON");
    let content = &value["message"]["content"];
    assert_eq!(content[0]["type"], "image");
    assert_eq!(content[0]["source"], serde_json::json!({"type": "base64", "media_type": "image/png", "data": "AQID"}));
    assert_eq!(content[1]["text"], "hi");
}

#[test]
fn text_delta_is_text() {
    let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"{\"items\""}}}"#;
    assert_eq!(ClaudeCli.parse_line(line), LineEvent::Delta(Delta::Text("{\"items\"".into())));
}

#[test]
fn thinking_delta_is_thinking() {
    let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hm"}}}"#;
    assert_eq!(ClaudeCli.parse_line(line), LineEvent::Delta(Delta::Thinking));
}

#[test]
fn other_lines_are_ignored() {
    assert_eq!(ClaudeCli.parse_line(r#"{"type":"system","subtype":"init"}"#), LineEvent::Ignore);
    assert_eq!(ClaudeCli.parse_line("not json"), LineEvent::Ignore);
}

#[test]
fn result_line_gives_text_tokens_and_cost() {
    let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"a\":1}","session_id":"s1","total_cost_usd":0.0024,"usage":{"input_tokens":2,"cache_creation_input_tokens":100,"cache_read_input_tokens":10,"output_tokens":219}}"#;
    let LineEvent::Done(Ok(reply)) = ClaudeCli.parse_line(line) else { panic!("expected a reply") };
    assert_eq!((reply.text.as_str(), reply.input_tokens, reply.output_tokens), ("{\"a\":1}", 112, 219));
    assert_eq!(reply.attributes["cost_usd"], "0.002400");
    assert_eq!(reply.attributes["session_id"], "s1");
}

#[test]
fn structured_output_wins_over_result_text() {
    let line = r#"{"type":"result","is_error":false,"result":"Paris it is","structured_output":{"city":"Paris"}}"#;
    let LineEvent::Done(Ok(reply)) = ClaudeCli.parse_line(line) else { panic!("expected a reply") };
    assert_eq!(reply.text, r#"{"city":"Paris"}"#);
}

#[test]
fn error_result_is_failed() {
    let line = r#"{"type":"result","is_error":true,"result":"model not found"}"#;
    assert_eq!(ClaudeCli.parse_line(line), LineEvent::Done(Err(LlmError::Failed("claude: model not found".into()))));
}
