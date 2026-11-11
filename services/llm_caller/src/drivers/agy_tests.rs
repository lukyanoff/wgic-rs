//! Lines recorded from a real `agy -p --output-format stream-json` run.

use super::*;

#[test]
fn args_are_sandboxed_and_end_with_the_prompt() {
    let args = AgyDriver::new("agy".into(), "gemini-3.8-flash-low".into(), "low".into()).args("hi".into());
    let joined = args.join(" ");
    assert!(
        joined.contains("--sandbox --mode plan --disable-slash-commands --model gemini-3.8-flash-low --effort low")
    );
    assert_eq!(args[args.len() - 2..], ["-p", "hi"]);
}

#[test]
fn text_deltas_and_tool_steps() {
    let done =
        r#"{"event":"step_update","step_update":{"state":"DONE","step_type":"agent_response","text_delta":"}\n"}}"#;
    let tool = r#"{"event":"step_update","step_update":{"state":"ACTIVE","step_type":"tool","tool_name":"view_file"}}"#;
    let user = r#"{"event":"step_update","step_update":{"state":"DONE","step_type":"user_input"}}"#;
    assert_eq!(parse_line(done), LineEvent::Text("}\n".into()));
    assert_eq!(parse_line(tool), LineEvent::Thinking);
    assert_eq!(parse_line(user), LineEvent::Ignore);
}

#[test]
fn result_gives_the_response_or_an_error() {
    let ok = r#"{"event":"result","result":{"status":"SUCCESS","response":"Paris.\n"}}"#;
    assert_eq!(parse_line(ok), LineEvent::Done(Ok("Paris.".into())));
    let LineEvent::Done(Err(error)) = parse_line(r#"{"event":"result","result":{"status":"ERROR","error":"quota"}}"#)
    else {
        panic!("expected an error")
    };
    assert_eq!(error.message, "agy: ERROR quota");
}
