//! Unit tests for `http.rs`: request body, SSE chunks, status mapping.

use connectrpc::ErrorCode;
use tokio::sync::mpsc;

use super::*;
use crate::drivers::Image;

fn driver() -> HttpDriver {
    let settings = LlmSettings {
        driver: "openrouter".into(),
        model: "vendor/model".into(),
        effort: "low".into(),
        ..Default::default()
    };
    HttpDriver::new(&Deps::in_process(&[]), &settings)
}

fn request(schema: &str) -> CallRequest {
    let message =
        |role: &str, content: &str| ChatMessage { role: role.into(), content: content.into(), ..Default::default() };
    CallRequest {
        messages: vec![message("system", "JSON only"), message("user", "hi")],
        json_schema: schema.into(),
        ..Default::default()
    }
}

/// Apply `chunks` in order; return the reply and the reported deltas.
fn read(chunks: &[&str]) -> (LlmReply, Vec<Delta>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut reply = LlmReply::default();
    for chunk in chunks {
        apply(chunk, &mut reply, &tx).unwrap();
    }
    let mut deltas = Vec::new();
    while let Ok(delta) = rx.try_recv() {
        deltas.push(delta);
    }
    (reply, deltas)
}

#[test]
fn preset_picks_url_and_key() {
    let driver = driver();
    assert_eq!(
        (driver.url.as_str(), driver.api_key_env),
        ("https://openrouter.ai/api/v1/chat/completions", "OPENROUTER_API_KEY")
    );
}

#[test]
fn body_has_model_messages_effort_and_schema() {
    let body = driver().body(&request(r#"{"type":"array"}"#));
    assert_eq!((body["model"].as_str(), body["reasoning_effort"].as_str()), (Some("vendor/model"), Some("low")));
    assert_eq!(body["messages"][0], json!({ "role": "system", "content": "JSON only" }));
    assert_eq!(body["response_format"]["json_schema"]["schema"]["type"], "array");
    assert_eq!(driver().body(&request(""))["response_format"], json!({ "type": "json_object" }));
}

#[test]
fn image_becomes_a_data_url_part_before_the_text() {
    let mut req = request("");
    req.messages[1].images.push(Image { data: vec![1, 2, 3], mime_type: "image/jpeg".into(), ..Default::default() });
    let content = &driver().body(&req)["messages"][1]["content"];
    assert_eq!(content[0], json!({"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,AQID"}}));
    assert_eq!(content[1], json!({"type": "text", "text": "hi"}));
}

#[test]
fn chunks_give_text_thinking_and_usage() {
    let (reply, deltas) = read(&[
        r#"{"choices":[{"delta":{"role":"assistant"}}]}"#,
        r#"{"choices":[{"delta":{"content":"","reasoning":"hm"}}]}"#,
        r#"{"choices":[{"delta":{"content":"[1,"}}]}"#,
        r#"{"choices":[{"delta":{"content":" 2]"}}]}"#,
        r#"{"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":7}}"#,
    ]);
    assert_eq!(reply, LlmReply { text: "[1, 2]".into(), input_tokens: 5, output_tokens: 7 });
    assert_eq!(deltas, [Delta::Thinking, Delta::Text("[1,".into()), Delta::Text(" 2]".into())]);
}

#[test]
fn error_chunk_fails_the_call() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let error = apply(r#"{"error":{"message":"No endpoints found"}}"#, &mut LlmReply::default(), &tx).unwrap_err();
    assert_eq!(error.message.as_deref(), Some("provider error: No endpoints found"));
}

#[test]
fn lines_survive_chunk_boundaries() {
    let bytes = "data: é\r\n".as_bytes();
    let mut buffer = bytes[..7].to_vec(); // half of 'é'
    assert_eq!(take_line(&mut buffer), None);
    buffer.extend_from_slice(&bytes[7..]);
    assert_eq!(take_line(&mut buffer).as_deref(), Some("data: é"));
}

#[test]
fn status_codes_map_to_connect_codes() {
    let unauthorized = status_error(401, r#"{"error":{"message":"Incorrect API key"}}"#);
    assert_eq!(
        (unauthorized.code, unauthorized.message.as_deref()),
        (ErrorCode::Unauthenticated, Some("HTTP 401: Incorrect API key"))
    );
    assert_eq!(status_error(429, "").code, ErrorCode::ResourceExhausted);
    assert_eq!(status_error(503, "busy").code, ErrorCode::Unavailable);
    assert_eq!(status_error(400, "bad model").code, ErrorCode::Internal);
}
