//! Unit tests for `agy.rs`, with lines recorded from a real `agy -p` run.

use super::*;
use crate::config::DriverKind;
use crate::domain::{ChatMessage, Role};

fn request(schema: Option<&str>) -> LlmRequest {
    let messages = vec![
        ChatMessage { role: Role::System, content: "Answer in JSON.".into() },
        ChatMessage { role: Role::User, content: "hi".into() },
    ];
    LlmRequest { messages, json_schema: schema.map(String::from) }
}

#[test]
fn args_are_sandboxed_and_end_with_the_prompt() {
    let settings = LlmSettings::new("x", DriverKind::AgyCli, "gemini-3.8-flash-low");
    let args = AgyCli.args(&settings, &request(Some("{}")));
    let joined = args.join(" ");
    assert!(joined.contains("--sandbox --mode plan"));
    assert!(joined.contains("--model gemini-3.8-flash-low --effort medium --json-schema {}"));
    assert_eq!(args[args.len() - 2], "-p");
    assert_eq!(args[args.len() - 1], "Answer in JSON.\n\nhi");
    assert_eq!(AgyCli.stdin(&request(None)), None);
}

#[test]
fn text_delta_is_text_also_on_the_done_step() {
    let active =
        r#"{"event":"step_update","step_update":{"state":"ACTIVE","step_type":"agent_response","text_delta":"{"}}"#;
    let done =
        r#"{"event":"step_update","step_update":{"state":"DONE","step_type":"agent_response","text_delta":"}\n"}}"#;
    assert_eq!(AgyCli.parse_line(active), LineEvent::Delta(Delta::Text("{".into())));
    assert_eq!(AgyCli.parse_line(done), LineEvent::Delta(Delta::Text("}\n".into())));
}

#[test]
fn tool_step_is_thinking() {
    let line =
        r#"{"event":"step_update","step_update":{"state":"ACTIVE","step_type":"tool","tool_name":"search_web"}}"#;
    assert_eq!(AgyCli.parse_line(line), LineEvent::Delta(Delta::Thinking));
}

#[test]
fn init_and_user_input_are_ignored() {
    assert_eq!(AgyCli.parse_line(r#"{"event":"init","init":{}}"#), LineEvent::Ignore);
    let user = r#"{"event":"step_update","step_update":{"state":"DONE","step_type":"user_input"}}"#;
    assert_eq!(AgyCli.parse_line(user), LineEvent::Ignore);
}

#[test]
fn result_gives_structured_output_and_tokens() {
    let line = r#"{"event":"result","result":{"conversation_id":"c1","status":"SUCCESS","response":"Paris.\n{\"city\":\"Paris\"}\n","num_turns":2,"structured_output":{"city":"Paris"},"usage":{"input_tokens":9107,"output_tokens":6}}}"#;
    let LineEvent::Done(Ok(reply)) = AgyCli.parse_line(line) else { panic!("expected a reply") };
    assert_eq!((reply.text.as_str(), reply.input_tokens, reply.output_tokens), (r#"{"city":"Paris"}"#, 9107, 6));
    assert_eq!((reply.attributes["conversation_id"].as_str(), reply.attributes["num_turns"].as_str()), ("c1", "2"));
}

#[test]
fn result_without_structured_output_uses_response() {
    let line = r#"{"event":"result","result":{"status":"SUCCESS","response":"{\"n\": 2}\n"}}"#;
    let LineEvent::Done(Ok(reply)) = AgyCli.parse_line(line) else { panic!("expected a reply") };
    assert_eq!(reply.text, "{\"n\": 2}\n");
}

#[test]
fn failed_status_is_failed() {
    let line = r#"{"event":"result","result":{"status":"ERROR","error":"quota exceeded"}}"#;
    assert_eq!(AgyCli.parse_line(line), LineEvent::Done(Err(LlmError::Failed("agy: ERROR quota exceeded".into()))));
}
