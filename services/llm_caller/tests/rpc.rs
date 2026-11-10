//! Integration: the real generated client → full connect stack → LlmCallerService, in-process,
//! with a fake driver (no LLM needed). Checks the streamed order, the result, metrics and error codes.

use std::sync::Arc;

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::llm_caller::v1::{
    CallRequest, CallResponse, ChatMessage, LlmCallerServiceClient, call_response::Event,
};
use kernel::testing::MemoryMetrics;
use kernel::{Channel, Deps};
use llm_caller::config::{DriverKind, LlmSettings};
use llm_caller::domain::{Delta, DriverFuture, LlmDriver, LlmError, LlmReply, LlmRequest};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------------------------

/// Writes `pieces` one by one (with a pause, so progress isn't throttled away), then answers.
struct FakeDriver {
    pieces: Vec<&'static str>,
    error: Option<LlmError>,
}

impl LlmDriver for FakeDriver {
    fn call(&self, _request: LlmRequest, deltas: mpsc::UnboundedSender<Delta>) -> DriverFuture {
        let pieces = self.pieces.clone();
        let error = self.error.clone();
        Box::pin(async move {
            if let Some(error) = error {
                return Err(error);
            }
            let _ = deltas.send(Delta::Thinking);
            for piece in &pieces {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                let _ = deltas.send(Delta::Text(piece.to_string()));
            }
            Ok(LlmReply { text: pieces.concat(), input_tokens: 10, output_tokens: 3, ..Default::default() })
        })
    }
}

fn client(driver: FakeDriver, metrics: &MemoryMetrics) -> LlmCallerServiceClient<Channel> {
    let mut settings = LlmSettings::new("llm-test", DriverKind::OpenRouter, "fake/model");
    settings.temperature = Some(0.5);
    let service_deps = Deps::in_process(&[]).with_metrics(metrics.clone());
    let module = llm_caller::module_with(&service_deps, Arc::new(settings), Arc::new(driver));
    let caller = Deps::in_process(&[module]);
    LlmCallerServiceClient::new(caller.channel(), caller.client_config())
}

fn request(role: &str, content: &str) -> CallRequest {
    let message = ChatMessage { role: role.into(), content: content.into(), ..Default::default() };
    CallRequest { messages: vec![message], ..Default::default() }
}

/// All stream messages, or the error that ended the stream.
async fn collect(
    client: &LlmCallerServiceClient<Channel>,
    req: CallRequest,
) -> Result<Vec<CallResponse>, ConnectError> {
    let mut stream = client.call(req).await?;
    let mut all = Vec::new();
    while let Some(message) = stream.message().await? {
        all.push(message.to_owned_message());
    }
    Ok(all)
}

async fn error_code(driver: FakeDriver, req: CallRequest) -> ErrorCode {
    let metrics = MemoryMetrics::new();
    collect(&client(driver, &metrics), req).await.unwrap_err().code
}

fn fails_with(error: LlmError) -> FakeDriver {
    FakeDriver { pieces: vec![], error: Some(error) }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn streams_progress_then_the_json_result() {
    let metrics = MemoryMetrics::new();
    let driver = FakeDriver { pieces: vec!["{\"city\":", " \"Paris\"}"], error: None };
    let all = collect(&client(driver, &metrics), request("user", "capital of France?")).await.unwrap();

    let Some(Event::Result(result)) = &all.last().unwrap().event else { panic!("last message must be the result") };
    assert_eq!(result.json, r#"{"city":"Paris"}"#);
    let m = &result.metadata;
    assert_eq!((m.caller_id.as_str(), m.driver.as_str(), m.model.as_str()), ("llm-test", "openrouter", "fake/model"));
    assert_eq!((m.temperature, m.input_tokens, m.output_tokens), (Some(0.5), 10, 3));

    let stages: Vec<String> = all[..all.len() - 1]
        .iter()
        .map(|message| match &message.event {
            Some(Event::Progress(progress)) => progress.stage.clone(),
            _ => panic!("only progress before the result"),
        })
        .collect();
    assert_eq!(stages.first().map(String::as_str), Some("started"));
    assert!(stages.iter().any(|stage| stage == "thinking"));
    assert!(stages.iter().any(|stage| stage == "writing"));
    assert_eq!(metrics.sum("llm_caller.calls"), 1.0);
}

#[tokio::test]
async fn partial_text_grows() {
    let metrics = MemoryMetrics::new();
    let driver = FakeDriver { pieces: vec!["[1,", " 2]"], error: None };
    let all = collect(&client(driver, &metrics), request("user", "numbers")).await.unwrap();
    let partials: Vec<String> = all
        .iter()
        .filter_map(|message| match &message.event {
            Some(Event::Progress(progress)) if progress.stage == "writing" => Some(progress.partial_text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(partials, ["[1,", "[1, 2]"]);
}

#[tokio::test]
async fn unknown_role_is_invalid_argument() {
    assert_eq!(
        error_code(fails_with(LlmError::Failed("x".into())), request("tool", "hi")).await,
        ErrorCode::InvalidArgument
    );
}

#[tokio::test]
async fn non_json_answer_is_data_loss() {
    let driver = FakeDriver { pieces: vec!["sorry"], error: None };
    assert_eq!(error_code(driver, request("user", "hi")).await, ErrorCode::DataLoss);
}

#[tokio::test]
async fn driver_errors_map_to_codes() {
    let cases = [
        (LlmError::NotConfigured("no key".into()), ErrorCode::FailedPrecondition),
        (LlmError::Unauthenticated("bad key".into()), ErrorCode::Unauthenticated),
        (LlmError::RateLimited("slow".into()), ErrorCode::ResourceExhausted),
        (LlmError::Unavailable("down".into()), ErrorCode::Unavailable),
        (LlmError::Failed("boom".into()), ErrorCode::Internal),
    ];
    for (error, code) in cases {
        assert_eq!(error_code(fails_with(error), request("user", "hi")).await, code);
    }
}

#[tokio::test]
async fn failures_are_counted() {
    let metrics = MemoryMetrics::new();
    let client = client(fails_with(LlmError::RateLimited("slow".into())), &metrics);
    let _ = collect(&client, request("user", "hi")).await;
    assert_eq!(metrics.sum("llm_caller.failures"), 1.0);
}
