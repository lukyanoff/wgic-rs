//! Unit tests for `agy.rs`, with lines recorded from a real `agy -p` run.

use super::*;
use crate::drivers::{ChatMessage, Image};

fn request(schema: &str) -> CallRequest {
    let message =
        |role: &str, content: &str| ChatMessage { role: role.into(), content: content.into(), ..Default::default() };
    let messages = vec![message("system", "Answer in JSON."), message("user", "hi")];
    CallRequest { messages, json_schema: schema.into(), ..Default::default() }
}

fn settings() -> LlmSettings {
    LlmSettings { model: "gemini-3.8-flash-low".into(), effort: "medium".into(), ..Default::default() }
}

#[test]
fn args_are_sandboxed_and_end_with_the_prompt() {
    let args = AgyCli.args(&settings(), &request("{}"));
    let joined = args.join(" ");
    assert!(joined.contains("--sandbox --mode plan"));
    assert!(joined.contains("--model gemini-3.8-flash-low --effort medium --json-schema {}"));
    assert_eq!(args[args.len() - 2..], ["-p", "Answer in JSON.\n\nhi"]);
    assert_eq!(AgyCli.stdin(&request("")), None);
}

#[test]
fn text_deltas_and_tool_steps() {
    let done =
        r#"{"event":"step_update","step_update":{"state":"DONE","step_type":"agent_response","text_delta":"}\n"}}"#;
    let tool =
        r#"{"event":"step_update","step_update":{"state":"ACTIVE","step_type":"tool","tool_name":"search_web"}}"#;
    let user = r#"{"event":"step_update","step_update":{"state":"DONE","step_type":"user_input"}}"#;
    assert!(matches!(AgyCli.parse_line(done), LineEvent::Delta(Delta::Text(t)) if t == "}\n"));
    assert!(matches!(AgyCli.parse_line(tool), LineEvent::Delta(Delta::Thinking)));
    assert!(matches!(AgyCli.parse_line(user), LineEvent::Ignore));
}

#[test]
fn result_gives_structured_output_and_tokens() {
    let line = r#"{"event":"result","result":{"status":"SUCCESS","response":"Paris.\n{\"city\":\"Paris\"}\n","structured_output":{"city":"Paris"},"usage":{"input_tokens":9107,"output_tokens":6}}}"#;
    let LineEvent::Done(Ok(reply)) = AgyCli.parse_line(line) else { panic!("expected a reply") };
    assert_eq!(reply, LlmReply { text: r#"{"city":"Paris"}"#.into(), input_tokens: 9107, output_tokens: 6 });
}

#[test]
fn failed_status_is_internal() {
    let line = r#"{"event":"result","result":{"status":"ERROR","error":"quota exceeded"}}"#;
    let LineEvent::Done(Err(error)) = AgyCli.parse_line(line) else { panic!("expected an error") };
    assert_eq!(error.message.as_deref(), Some("agy: ERROR quota exceeded"));
}

#[test]
fn images_become_files_named_in_the_prompt() {
    let mut req = request("");
    req.messages[1].images.push(Image { data: vec![7], mime_type: "image/jpeg".into(), ..Default::default() });
    assert_eq!(AgyCli.files(&req), vec![("image-1.jpg".to_string(), vec![7])]);
    let prompt = AgyCli.args(&settings(), &req).pop().expect("prompt");
    assert!(prompt.contains("the files image-1.jpg in the current folder"), "{prompt}");
}
