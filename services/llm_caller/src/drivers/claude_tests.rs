//! Lines recorded from a real `claude -p --output-format stream-json` run.

use super::*;
use crate::drivers::ImageArg;

#[test]
fn args_carry_model_and_system_prompt() {
    let driver = ClaudeDriver::new("claude".into(), "claude-haiku-5-5".into());
    let args = driver.args(&PromptArgs { system_prompt: "Be brief.".into(), ..Default::default() }).join(" ");
    assert!(args.starts_with("-p --input-format stream-json --output-format stream-json"));
    assert!(args.ends_with("--model claude-haiku-5-5 --system-prompt Be brief."));
}

#[test]
fn images_go_to_stdin_first_as_base64_blocks() {
    let args = PromptArgs {
        user_prompt: "hi".into(),
        images: vec![ImageArg { data: vec![1, 2, 3], mime_type: "image/png".into() }],
        ..Default::default()
    };
    let value: Value = serde_json::from_str(&stdin_line(&args)).expect("one JSON line");
    let content = &value["message"]["content"];
    assert_eq!(content[0]["source"], json!({"type": "base64", "media_type": "image/png", "data": "AQID"}));
    assert_eq!(content[1], json!({"type": "text", "text": "hi"}));
}

#[test]
fn deltas_are_text_or_thinking() {
    let text = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"A desk"}}}"#;
    let thinking = r#"{"type":"stream_event","event":{"delta":{"type":"thinking_delta","thinking":"hm"}}}"#;
    assert_eq!(parse_line(text), LineEvent::Text("A desk".into()));
    assert_eq!(parse_line(thinking), LineEvent::Thinking);
    assert_eq!(parse_line(r#"{"type":"system","subtype":"init"}"#), LineEvent::Ignore);
}

#[test]
fn result_line_gives_the_answer_or_an_error() {
    assert_eq!(
        parse_line(r#"{"type":"result","is_error":false,"result":"A desk."}"#),
        LineEvent::Done(Ok("A desk.".into()))
    );
    let LineEvent::Done(Err(error)) = parse_line(r#"{"type":"result","is_error":true,"result":"model not found"}"#)
    else {
        panic!("expected an error")
    };
    assert_eq!(error.message, "claude: model not found");
}
