//! Integration: the two driver base classes against stand-ins for the real thing:
//! - `CliCaller` runs a small shell script that prints recorded `claude` stream-json lines;
//! - `HttpCaller` talks to a local TCP server that answers with recorded OpenAI SSE events.

use std::sync::Arc;
use std::time::Duration;

use llm_caller::config::{DriverKind, LlmSettings};
use llm_caller::domain::{ChatMessage, Delta, Image, LlmDriver, LlmError, LlmReply, LlmRequest, Role};
use llm_caller::drivers::cli::agy::AgyCli;
use llm_caller::drivers::cli::base::CliCaller;
use llm_caller::drivers::cli::claude::ClaudeCli;
use llm_caller::drivers::http::base::HttpCaller;
use llm_caller::drivers::http::openai::OpenAi;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn request() -> LlmRequest {
    let messages = vec![ChatMessage::new(Role::User, "hi")];
    LlmRequest { messages, json_schema: None }
}

/// Run one call; return the result and every delta the driver reported.
async fn run(driver: &dyn LlmDriver) -> (Result<LlmReply, LlmError>, Vec<Delta>) {
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
    slow: String,
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
            slow: script("claude_slow", "sleep 30"),
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

fn claude_with(cli_path: String) -> CliCaller {
    let mut settings = LlmSettings::new("t", DriverKind::ClaudeCli, "m");
    settings.cli_path = Some(cli_path);
    settings.timeout = Duration::from_secs(5);
    CliCaller::new(Arc::new(ClaudeCli), settings)
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

fn openai_at(base_url: String, api_key: Option<&str>) -> HttpCaller {
    let mut settings = LlmSettings::new("t", DriverKind::OpenAi, "m");
    settings.base_url = Some(base_url);
    HttpCaller::new(Arc::new(OpenAi), settings, api_key.map(String::from))
}

// ---------------------------------------------------------------------------------------------
// CliCaller
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
    let mut settings = LlmSettings::new("t", DriverKind::AgyCli, "m");
    settings.cli_path = Some(scripts().agy_image.clone());
    let mut message = ChatMessage::new(Role::User, "what is this?");
    message.images.push(Image { data: vec![0x89, b'P', b'N', b'G'], mime_type: "image/png".into() });
    let request = LlmRequest { messages: vec![message], json_schema: None };
    let (tx, _rx) = mpsc::unbounded_channel();
    let reply = CliCaller::new(Arc::new(AgyCli), settings).call(request, tx).await.unwrap();
    assert_eq!(reply.text, r#"{"seen":"image-1.png"}"#);
}

#[tokio::test]
async fn cli_failure_reports_stderr() {
    let (result, _) = run(&claude_with(scripts().fail.clone())).await;
    let Err(LlmError::Failed(message)) = result else { panic!("expected Failed, got {result:?}") };
    assert!(message.contains("invalid model: m"), "{message}");
}

#[tokio::test]
async fn missing_cli_is_not_configured() {
    scripts(); // see `scripts`: no process starts while they are being written
    let (result, _) = run(&claude_with("/no/such/claude".into())).await;
    assert!(matches!(result, Err(LlmError::NotConfigured(_))), "{result:?}");
}

#[tokio::test]
async fn slow_cli_times_out() {
    let mut caller_settings = LlmSettings::new("t", DriverKind::ClaudeCli, "m");
    caller_settings.cli_path = Some(scripts().slow.clone());
    caller_settings.timeout = Duration::from_millis(300);
    let (result, _) = run(&CliCaller::new(Arc::new(ClaudeCli), caller_settings)).await;
    assert!(matches!(result, Err(LlmError::Unavailable(_))), "{result:?}");
}

// ---------------------------------------------------------------------------------------------
// HttpCaller
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn http_streams_sse_text_and_usage() {
    let events = [
        r#"{"type":"response.created"}"#,
        r#"{"type":"response.output_text.delta","delta":"[1,"}"#,
        r#"{"type":"response.output_text.delta","delta":" 2]"}"#,
        r#"{"type":"response.completed","response":{"usage":{"input_tokens":5,"output_tokens":6}}}"#,
    ];
    let body: String = events.iter().map(|e| format!("event: x\ndata: {e}\n\n")).collect();
    let base = fake_server("200 OK", "text/event-stream", body).await;
    let (result, deltas) = run(&openai_at(base, Some("key"))).await;
    let reply = result.unwrap();
    assert_eq!((reply.text.as_str(), reply.input_tokens, reply.output_tokens), ("[1, 2]", 5, 6));
    assert_eq!(deltas.len(), 2);
}

#[tokio::test]
async fn http_401_is_unauthenticated() {
    let body = r#"{"error":{"message":"Incorrect API key provided"}}"#.to_string();
    let base = fake_server("401 Unauthorized", "application/json", body).await;
    let (result, _) = run(&openai_at(base, Some("bad"))).await;
    assert_eq!(result.unwrap_err(), LlmError::Unauthenticated("HTTP 401: Incorrect API key provided".into()));
}

#[tokio::test]
async fn missing_api_key_is_not_configured() {
    let (result, _) = run(&openai_at("http://127.0.0.1:9".into(), None)).await;
    assert_eq!(result.unwrap_err(), LlmError::NotConfigured("OPENAI_API_KEY is not set".into()));
}
