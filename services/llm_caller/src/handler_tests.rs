//! Unit tests for the pure parts of `handler.rs`.

use connectrpc::ErrorCode;

use super::*;

fn chat(messages: &[(&str, &str)], schema: &str) -> CallRequest {
    let messages = messages
        .iter()
        .map(|(role, content)| ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
            ..Default::default()
        })
        .collect();
    CallRequest { messages, json_schema: schema.into(), ..Default::default() }
}

#[test]
fn roles_are_lowercased_and_checked() {
    assert_eq!(checked(chat(&[("User", "hi")], "")).unwrap().messages[0].role, "user");
    assert_eq!(checked(chat(&[("tool", "hi")], "")).unwrap_err().code, ErrorCode::InvalidArgument);
}

#[test]
fn chat_needs_user_text_and_a_json_schema() {
    assert!(checked(chat(&[("system", "be nice")], "")).is_err());
    assert!(checked(chat(&[("user", "  ")], "")).is_err());
    assert!(checked(chat(&[("user", "hi")], "{not json")).is_err());
}

#[test]
fn json_instruction_comes_first_with_the_schema() {
    let request = with_json_instruction(chat(&[("user", "hi")], r#"{"type":"object"}"#));
    assert_eq!(request.messages[0].role, "system");
    assert!(request.messages[0].content.contains(r#"JSON Schema:\n{"type":"object"}"#.replace("\\n", "\n").as_str()));
    assert_eq!(request.messages[1].content, "hi");
}

#[test]
fn json_is_found_and_compacted() {
    assert_eq!(extract_json("{ \"a\": 1 }\n").as_deref(), Some(r#"{"a":1}"#));
    assert_eq!(extract_json("```json\n{\"items\": [1, 2]}\n```").as_deref(), Some(r#"{"items":[1,2]}"#));
    assert_eq!(extract_json("The city is **Paris**.\n{\"city\":\"Paris\"}").as_deref(), Some(r#"{"city":"Paris"}"#));
    assert_eq!(extract_json("[1, 2]").as_deref(), Some("[1,2]"));
}

#[test]
fn text_without_json_is_none() {
    assert_eq!(extract_json("no json here"), None);
    assert_eq!(extract_json("} backwards {"), None);
    assert_eq!(extract_json("{ broken"), None);
}

#[test]
fn progress_is_throttled_except_on_stage_changes() {
    let t0 = Instant::now();
    let mut progress = Progress::new(t0);
    assert_eq!(progress.next(Delta::Thinking, t0).unwrap().stage, "thinking");
    let first = progress.next(Delta::Text("Hel".into()), t0).unwrap(); // stage change → sent
    assert_eq!((first.stage.as_str(), first.partial_text.as_str()), ("writing", "Hel"));
    assert_eq!(progress.next(Delta::Text("lo".into()), t0 + Duration::from_millis(10)), None);
    let later = progress.next(Delta::Text("!".into()), t0 + Duration::from_millis(300)).unwrap();
    assert_eq!((later.partial_text.as_str(), later.elapsed_ms), ("Hello!", 300));
}
