//! Integration: the two driver base classes against stand-ins for the real thing:
//! - `CliDriver` runs a small shell script that prints recorded `claude` / `agy` lines;
//! - `HttpDriver` talks to a local TCP server that answers with recorded chat-completions SSE events.

use connectrpc::{ConnectError, ErrorCode};
use kernel::Deps;
use llm_caller::LlmSettings;
use llm_caller::drivers::{
    AgyCli, CallRequest, ChatMessage, ClaudeCli, CliDriver, Delta, HttpDriver, Image, LlmDriver, LlmReply,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn user(content: &str) -> ChatMessage {
    ChatMessage { role: "user".into(), content: content.into(), ..Default::default() }
}

fn request() -> CallRequest {
    CallRequest { messages: vec![user("hi")], ..Default::default() }
}

fn settings(driver: &str) -> LlmSettings {
    LlmSettings { id: "t".into(), driver: driver.into(), model: "m".into(), ..Default::default() }
}

/// Run one call; return the result and every delta the driver reported.
async fn run(driver: &dyn LlmDriver) -> (Result<LlmReply, ConnectError>, Vec<Delta>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = driver.call(request(), tx).await;
    let mut deltas = Vec::new();
    while let Ok(delta) = rx.try_recv() {
        deltas.push(delta);
    }
    (result, deltas)
}

/// Recorded `claude` stream-json output of a successful call.
const CLAUDE_OK_LINES: [&str; 4] = [
    r#"{"type":"system","subtype":"init"}"#,
    r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"{\"a\":"}}}"#,
    r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"1}"}}}"#,
    r#"{"type":"result","is_error":false,"result":"{\"a\":1}","usage":{"input_tokens":3,"output_tokens":4}}"#,
];

const AGY_IMAGE_SCRIPT: &str = r#"if [ -s image-1.png ]; then
  echo '{"event":"result","result":{"status":"SUCCESS","response":"{\"seen\":\"image-1.png\"}"}}'
else
  echo '{"event":"result","result":{"status":"ERROR","error":"no image file"}}'
fi"#;

/// Paths of the fake CLIs.
struct Scripts {
    ok: String,
    fail: String,
    /// A fake agy that answers `{"seen":"image-1.png"}` only when that file is in its folder.
    agy_image: String,
}

/// All fake CLIs, written once before any test starts a process. Writing a script while another
/// thread forks lets the child inherit the open file, and running the script then fails with
/// "Text file busy" (ETXTBSY). So every CLI test calls this first.
fn scripts() -> &'static Scripts {
    static SCRIPTS: std::sync::OnceLock<Scripts> = std::sync::OnceLock::new();
    SCRIPTS.get_or_init(|| {
        let echo: Vec<String> = CLAUDE_OK_LINES.iter().map(|line| format!("echo '{line}'")).collect();
        Scripts {
            ok: script("claude_ok", &format!("cat > /dev/null\n{}", echo.join("\n"))),
            fail: script("claude_fail", "echo 'invalid model: m' >&2\nexit 3"),
            agy_image: script("agy_image", AGY_IMAGE_SCRIPT),
        }
    })
}

/// An executable script in a temp folder of this test run.
fn script(name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("llm_caller_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().into_owned()
}

fn claude_with(cli_path: String) -> CliDriver {
    let settings = LlmSettings { cli_path: Some(cli_path), ..settings("claude-cli") };
    CliDriver::new(ClaudeCli, &settings)
}

/// Serve one HTTP request with `status` and `body`; return the base URL.
async fn fake_server(status: &'static str, content_type: &'static str, body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        let head = format!("HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\nconnection: close\r\n\r\n");
        let _ = socket.write_all(head.as_bytes()).await;
        let _ = socket.write_all(body.as_bytes()).await;
    });
    format!("http://{addr}")
}

/// Read the headers and the `content-length` body, so the client is done sending.
async fn read_request(socket: &mut tokio::net::TcpStream) {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        if n == 0 {
            return;
        }
        data.extend_from_slice(&buf[..n]);
        if request_complete(&data) {
            return;
        }
    }
}

fn request_complete(data: &[u8]) -> bool {
    let text = String::from_utf8_lossy(data);
    let Some(end) = text.find("\r\n\r\n") else {
        return false;
    };
    let mut length = 0;
    for line in text[..end].lines() {
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse::<usize>().unwrap_or(0);
        }
    }
    data.len() >= end + 4 + length
}

fn openai_at(base_url: String, api_key: Option<&str>) -> HttpDriver {
    let settings = LlmSettings { base_url: Some(base_url), ..settings("openai") };
    // A blank value counts as unset, so a real OPENAI_API_KEY on this machine can't leak into the test.
    let deps = Deps::in_process(&[]).with_env([("OPENAI_API_KEY", api_key.unwrap_or(""))]);
    HttpDriver::new(&deps, &settings)
}

// ---------------------------------------------------------------------------------------------
// CliDriver
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn cli_streams_text_and_returns_the_result_line() {
    let (result, deltas) = run(&claude_with(scripts().ok.clone())).await;
    let reply = result.unwrap();
    assert_eq!((reply.text.as_str(), reply.input_tokens, reply.output_tokens), ("{\"a\":1}", 3, 4));
    assert_eq!(deltas, [Delta::Text("{\"a\":".into()), Delta::Text("1}".into())]);
}

#[tokio::test]
async fn agy_finds_the_image_as_a_file_in_its_folder() {
    let settings = LlmSettings { cli_path: Some(scripts().agy_image.clone()), ..settings("agy-cli") };
    let mut message = user("what is this?");
    message.images.push(Image {
        data: vec![0x89, b'P', b'N', b'G'],
        mime_type: "image/png".into(),
        ..Default::default()
    });
    let request = CallRequest { messages: vec![message], ..Default::default() };
    let (tx, _rx) = mpsc::unbounded_channel();
    let reply = CliDriver::new(AgyCli, &settings).call(request, tx).await.unwrap();
    assert_eq!(reply.text, r#"{"seen":"image-1.png"}"#);
}

#[tokio::test]
async fn cli_failure_reports_stderr() {
    let (result, _) = run(&claude_with(scripts().fail.clone())).await;
    let error = result.unwrap_err();
    assert_eq!(error.code, ErrorCode::Internal);
    assert!(error.message.as_deref().unwrap_or_default().contains("invalid model: m"), "{error:?}");
}

#[tokio::test]
async fn missing_cli_is_failed_precondition() {
    scripts(); // see `scripts`: no process starts while they are being written
    let (result, _) = run(&claude_with("/no/such/claude".into())).await;
    assert_eq!(result.unwrap_err().code, ErrorCode::FailedPrecondition);
}

// ---------------------------------------------------------------------------------------------
// HttpDriver
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn http_streams_sse_text_and_usage() {
    let events = [
        r#"{"choices":[{"delta":{"reasoning":"hmm"}}]}"#,
        r#"{"choices":[{"delta":{"content":"[1,"}}]}"#,
        r#"{"choices":[{"delta":{"content":" 2]"}}]}"#,
        r#"{"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":6}}"#,
        "[DONE]",
    ];
    let body: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
    let base = fake_server("200 OK", "text/event-stream", body).await;
    let (result, deltas) = run(&openai_at(base, Some("key"))).await;
    let reply = result.unwrap();
    assert_eq!((reply.text.as_str(), reply.input_tokens, reply.output_tokens), ("[1, 2]", 5, 6));
    assert_eq!(deltas, [Delta::Thinking, Delta::Text("[1,".into()), Delta::Text(" 2]".into())]);
}

#[tokio::test]
async fn http_401_is_unauthenticated() {
    let body = r#"{"error":{"message":"Incorrect API key provided"}}"#.to_string();
    let base = fake_server("401 Unauthorized", "application/json", body).await;
    let (result, _) = run(&openai_at(base, Some("bad"))).await;
    let error = result.unwrap_err();
    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert_eq!(error.message.as_deref(), Some("HTTP 401: Incorrect API key provided"));
}

#[tokio::test]
async fn missing_api_key_is_failed_precondition() {
    let (result, _) = run(&openai_at("http://127.0.0.1:9".into(), None)).await;
    let error = result.unwrap_err();
    assert_eq!(error.code, ErrorCode::FailedPrecondition);
    assert_eq!(error.message.as_deref(), Some("OPENAI_API_KEY is not set"));
}
