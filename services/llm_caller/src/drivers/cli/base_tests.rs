//! Unit tests for `base.rs`: chat rendering and how a finished process becomes a result.

use std::os::unix::process::ExitStatusExt;

use super::*;

fn msg(role: Role, content: &str) -> ChatMessage {
    ChatMessage { role, content: content.to_string() }
}

#[test]
fn system_messages_are_joined() {
    let messages = [msg(Role::System, "a"), msg(Role::User, "q"), msg(Role::System, "b")];
    assert_eq!(system_prompt(&messages), "a\n\nb");
}

#[test]
fn single_user_message_is_the_prompt() {
    assert_eq!(transcript(&[msg(Role::System, "s"), msg(Role::User, "hello")]), "hello");
}

#[test]
fn chat_becomes_a_transcript() {
    let text = transcript(&[msg(Role::User, "hi"), msg(Role::Assistant, "hello"), msg(Role::User, "and?")]);
    assert!(text.contains("\nUser: hi\n\nAssistant: hello\n\nUser: and?\n"));
}

#[test]
fn result_line_wins_over_exit_code() {
    let reply = LlmReply { text: "{}".into(), ..Default::default() };
    let result = finish(Some(Ok(reply.clone())), ExitStatus::from_raw(1 << 8), "");
    assert_eq!(result.unwrap(), reply);
}

#[test]
fn failed_exit_without_result_reports_stderr_tail() {
    let stderr = format!("{}the real reason", "x".repeat(1000));
    let Err(LlmError::Failed(message)) = finish(None, ExitStatus::from_raw(2 << 8), &stderr) else {
        panic!("expected Failed");
    };
    assert!(message.ends_with("the real reason"));
    assert!(message.len() < 600);
}
