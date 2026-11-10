//! E2E: LlmCallerService in the real `host` binary, over HTTP and over NATS (server streaming).
//!
//! The host runs the `openai` driver against a fake OpenAI server started by this test, so no
//! API key or internet is needed. The fake streams a JSON answer in pieces with pauses, so the
//! client sees progress messages before the result.

use std::time::Duration;

use connectrpc::client::CallOptions;
use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::llm_caller::v1::{
    CallRequest, CallResponse, ChatMessage, Image, LlmCallerServiceClient, call_response::Event,
};
use e2e::Host;
use e2e::fake_openai::{Seen, fake_openai};
use kernel::Deps;

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

async fn start_host() -> Host {
    start_host_with(false, Seen::default()).await
}

async fn start_host_with(hang: bool, seen: Seen) -> Host {
    let base_url = fake_openai(hang, seen).await;
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
    collect_with(deps, req, CallOptions::default()).await
}

async fn collect_with(deps: &Deps, req: CallRequest, options: CallOptions) -> Result<Vec<CallResponse>, ConnectError> {
    let client = LlmCallerServiceClient::new(deps.channel(), deps.client_config());
    let mut stream = client.call_with_options(req, options).await?;
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

#[tokio::test]
async fn image_reaches_the_provider_as_a_data_url() {
    let seen = Seen::default();
    let host = start_host_with(false, seen.clone()).await;
    let mut req = request("user", "What is in the picture?");
    let image = Image { data: vec![1, 2, 3], mime_type: "image/png".into(), ..Default::default() };
    req.messages[0].images.push(image);
    collect(&host.nats_deps().await, req).await.unwrap();
    let sent = seen.request.lock().expect("lock").clone();
    assert!(sent.contains(r#""url":"data:image/png;base64,AQID""#), "request was: {sent}");
}

/// The client's deadline travels over NATS; the llm_caller host ends the stream when it passes and drops
/// the provider call (the fake server sees the connection close).
#[tokio::test]
async fn deadline_over_nats_cancels_the_provider_call() {
    let seen = Seen::default();
    let host = start_host_with(true, seen.clone()).await;
    let options = CallOptions::default().with_timeout(Duration::from_secs(1));
    let error = collect_with(&host.nats_deps().await, request("user", "hi"), options).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::DeadlineExceeded);
    assert!(seen.wait_for_hang_up(Duration::from_secs(5)).await, "the provider call was not cancelled within 5 s");
}
