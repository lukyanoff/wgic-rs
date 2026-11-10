//! E2E: LlmCallerService in the real `host` binary, over HTTP and over NATS (server streaming).
//!
//! The host runs the `openai` driver against a fake OpenAI server started by this test, so no
//! API key or internet is needed. The fake streams a JSON answer in pieces with pauses, so the
//! client sees progress messages before the result.

use std::time::Duration;

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::llm_caller::v1::{
    CallRequest, CallResponse, ChatMessage, LlmCallerServiceClient, call_response::Event,
};
use e2e::Host;
use kernel::Deps;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// ---------------------------------------------------------------------------------------------
// Fake OpenAI server
// ---------------------------------------------------------------------------------------------

const PIECES: [&str; 3] = ["{\"city\":", " \"Paris\",", " \"country\": \"FR\"}"];

/// Answer every request with a streamed Responses API reply; return the base URL.
async fn fake_openai() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind fake server");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(answer(socket));
        }
    });
    format!("http://{addr}")
}

async fn answer(mut socket: TcpStream) {
    read_request(&mut socket).await;
    let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
    let _ = socket.write_all(head.as_bytes()).await;
    for piece in PIECES {
        let event = serde_event("response.output_text.delta", &format!("\"delta\":{piece:?}"));
        let _ = socket.write_all(event.as_bytes()).await;
        tokio::time::sleep(Duration::from_millis(300)).await; // > the service's progress interval
    }
    let usage = "\"response\":{\"usage\":{\"input_tokens\":7,\"output_tokens\":9}}";
    let _ = socket.write_all(serde_event("response.completed", usage).as_bytes()).await;
}

fn serde_event(kind: &str, fields: &str) -> String {
    format!("data: {{\"type\":\"{kind}\",{fields}}}\n\n")
}

/// Read until the headers and the `content-length` body are in.
async fn read_request(socket: &mut TcpStream) {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let Ok(n) = socket.read(&mut buf).await else { return };
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

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

async fn start_host() -> Host {
    let base_url = fake_openai().await;
    let env = [
        ("LLM_CALLER_ID", "llm-e2e"),
        ("LLM_CALLER_DRIVER", "openai"),
        ("LLM_CALLER_MODEL", "fake-model"),
        ("LLM_CALLER_EFFORT", "low"),
        ("LLM_CALLER_BASE_URL", base_url.as_str()),
        ("OPENAI_API_KEY", "test-key"),
    ];
    Host::start_with_env("llm_caller", &env).await
}

fn request(role: &str, content: &str) -> CallRequest {
    let message = ChatMessage { role: role.into(), content: content.into(), ..Default::default() };
    CallRequest { messages: vec![message], ..Default::default() }
}

async fn collect(deps: &Deps, req: CallRequest) -> Result<Vec<CallResponse>, ConnectError> {
    let client = LlmCallerServiceClient::new(deps.channel(), deps.client_config());
    let mut stream = client.call(req).await?;
    let mut all = Vec::new();
    while let Some(message) = stream.message().await? {
        all.push(message.to_owned_message());
    }
    Ok(all)
}

/// Progress (`started`, then `writing` with growing text), then the JSON result.
fn assert_progress_then_result(all: &[CallResponse]) {
    let (last, before) = all.split_last().expect("at least the result");
    let Some(Event::Result(result)) = &last.event else { panic!("last message must be the result") };
    assert_eq!(result.json, r#"{"city":"Paris","country":"FR"}"#);
    assert_eq!((result.metadata.caller_id.as_str(), result.metadata.driver.as_str()), ("llm-e2e", "openai"));
    assert_eq!((result.metadata.input_tokens, result.metadata.output_tokens), (7, 9));
    let mut partials = Vec::new();
    for message in before {
        let Some(Event::Progress(progress)) = &message.event else { panic!("only progress before the result") };
        partials.push(progress.partial_text.clone());
    }
    assert_eq!(partials.first().map(String::as_str), Some(""), "first progress is `started`");
    assert!(partials.len() >= 3, "expected several progress messages, got {partials:?}");
    assert!(partials.windows(2).all(|pair| pair[1].starts_with(&pair[0])), "partial text only grows");
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn streams_progress_and_json_over_http() {
    let host = start_host().await;
    let all = collect(&host.http_deps(), request("user", "Where is Paris?")).await.unwrap();
    assert_progress_then_result(&all);
}

#[tokio::test]
async fn streams_progress_and_json_over_nats() {
    let host = start_host().await;
    let all = collect(&host.nats_deps().await, request("user", "Where is Paris?")).await.unwrap();
    assert_progress_then_result(&all);
}

#[tokio::test]
async fn unknown_role_is_invalid_argument_over_nats() {
    let host = start_host().await;
    let error = collect(&host.nats_deps().await, request("robot", "hi")).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
}
