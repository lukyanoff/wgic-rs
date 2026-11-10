//! Unit tests for `cli.rs`: chat rendering, the work folder, and how a finished process becomes a result.

use std::os::unix::process::ExitStatusExt;

use super::*;

fn msg(role: &str, content: &str) -> ChatMessage {
    ChatMessage { role: role.into(), content: content.into(), ..Default::default() }
}

#[test]
fn system_messages_are_joined() {
    assert_eq!(system_prompt(&[msg("system", "a"), msg("user", "q"), msg("system", "b")]), "a\n\nb");
}

#[test]
fn single_message_is_the_prompt() {
    assert_eq!(transcript(&[msg("system", "s"), msg("user", "hello")]), "hello");
}

#[test]
fn chat_becomes_a_transcript() {
    let text = transcript(&[msg("user", "hi"), msg("assistant", "hello"), msg("user", "and?")]);
    assert!(text.contains("\nUser: hi\n\nAssistant: hello\n\nUser: and?\n"));
}

#[test]
fn result_line_wins_over_exit_code() {
    let reply = LlmReply { text: "{}".into(), ..Default::default() };
    assert_eq!(finish(Some(Ok(reply.clone())), ExitStatus::from_raw(1 << 8), "").unwrap(), reply);
}

#[test]
fn failed_exit_without_result_reports_the_end_of_stderr() {
    let stderr = format!("{}the real reason", "x".repeat(1000));
    let message = finish(None, ExitStatus::from_raw(2 << 8), &stderr).unwrap_err().message.unwrap();
    assert!(message.ends_with("the real reason") && message.len() < 600, "{message}");
}

#[test]
fn image_files_are_numbered_with_their_extension() {
    let mut message = msg("user", "look");
    for mime_type in ["image/png", "image/jpeg"] {
        message.images.push(Image { data: vec![1], mime_type: mime_type.into(), ..Default::default() });
    }
    let names: Vec<String> = image_files(&[message]).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["image-1.png", "image-2.jpg"]);
}

#[test]
fn work_dir_holds_the_files_and_is_deleted_on_drop() {
    let work_dir = WorkDir::create(vec![("a.png".into(), vec![9])]).unwrap();
    let path = work_dir.path.clone();
    assert_eq!(std::fs::read(path.join("a.png")).unwrap(), [9]);
    drop(work_dir);
    assert!(!path.exists());
}
