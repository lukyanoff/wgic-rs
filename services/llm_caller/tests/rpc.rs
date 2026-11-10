//! Integration: the real generated client → full connect stack → LlmCallerService, in-process,
//! with a fake driver (no LLM needed). Checks the streamed order, the result, metrics and error codes.

use std::sync::Arc;
use std::time::Duration;

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::llm_caller::v1::{
    CallRequest, CallResponse, ChatMessage, LlmCallerServiceClient, call_response::Event,
};
use kernel::testing::MemoryMetrics;
use kernel::{Channel, Deps};
use llm_caller::LlmSettings;
use llm_caller::drivers::{Delta, Deltas, DriverFuture, LlmDriver, LlmReply};

// ---------------------------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------------------------

/// Writes `pieces` one by one (with a pause, so progress isn't throttled away), then answers;
/// or fails at once with `error`.
#[derive(Default)]
struct FakeDriver {
    pieces: Vec<&'static str>,
    error: Option<ConnectError>,
}

impl LlmDriver for FakeDriver {
    fn call(&self, _request: CallRequest, deltas: Deltas) -> DriverFuture {
        let pieces = self.pieces.clone();
        let error = self.error.clone();
        Box::pin(async move {
            if let Some(error) = error {
                return Err(error);
            }
            let _ = deltas.send(Delta::Thinking);
            for piece in &pieces {
                tokio::time::sleep(Duration::from_millis(300)).await;
                let _ = deltas.send(Delta::Text(piece.to_string()));
            }
            Ok(LlmReply { text: pieces.concat(), input_tokens: 10, output_tokens: 3 })
        })
    }
}

fn answers(pieces: &[&'static str]) -> FakeDriver {
    FakeDriver { pieces: pieces.to_vec(), error: None }
}

fn fails_with(error: ConnectError) -> FakeDriver {
    FakeDriver { pieces: vec![], error: Some(error) }
}

fn client(driver: FakeDriver, metrics: &MemoryMetrics) -> LlmCallerServiceClient<Channel> {
    let settings = LlmSettings {
        id: "llm-test".into(),
        driver: "openrouter".into(),
        model: "fake/model".into(),
        ..Default::default()
    };
    let service_deps = Deps::in_process(&[]).with_metrics(metrics.clone());
    let module = llm_caller::module_with(&service_deps, settings, Arc::new(driver));
    let caller = Deps::in_process(&[module]);
    LlmCallerServiceClient::new(caller.channel(), caller.client_config())
}

fn request(role: &str, content: &str) -> CallRequest {
    let message = ChatMessage { role: role.into(), content: content.into(), ..Default::default() };
    CallRequest { messages: vec![message], ..Default::default() }
}

/// All stream messages, or the error that ended the stream.
async fn collect(
    driver: FakeDriver,
    metrics: &MemoryMetrics,
    req: CallRequest,
) -> Result<Vec<CallResponse>, ConnectError> {
    let mut stream = client(driver, metrics).call(req).await?;
    let mut all = Vec::new();
    while let Some(message) = stream.message().await? {
        all.push(message.to_owned_message());
    }
    Ok(all)
}

async fn error_code(driver: FakeDriver, req: CallRequest) -> ErrorCode {
    collect(driver, &MemoryMetrics::new(), req).await.unwrap_err().code
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn streams_progress_then_the_json_result() {
    let metrics = MemoryMetrics::new();
    let all = collect(answers(&["{\"city\":", " \"Paris\"}"]), &metrics, request("user", "capital of France?"))
        .await
        .unwrap();

    let Some(Event::Result(result)) = &all.last().unwrap().event else { panic!("last message must be the result") };
    assert_eq!(result.json, r#"{"city":"Paris"}"#);
    let m = &result.metadata;
    assert_eq!((m.caller_id.as_str(), m.driver.as_str(), m.model.as_str()), ("llm-test", "openrouter", "fake/model"));
    assert_eq!((m.input_tokens, m.output_tokens), (10, 3));

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
    assert_eq!(metrics.sum("llm_caller.output_tokens"), 3.0);
}

#[tokio::test]
async fn partial_text_grows() {
    let all = collect(answers(&["[1,", " 2]"]), &MemoryMetrics::new(), request("user", "numbers")).await.unwrap();
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
    assert_eq!(error_code(answers(&["{}"]), request("tool", "hi")).await, ErrorCode::InvalidArgument);
}

#[tokio::test]
async fn non_json_answer_is_data_loss() {
    assert_eq!(error_code(answers(&["sorry"]), request("user", "hi")).await, ErrorCode::DataLoss);
}

#[tokio::test]
async fn driver_errors_reach_the_client_unchanged() {
    let driver = fails_with(ConnectError::resource_exhausted("slow down"));
    let error = collect(driver, &MemoryMetrics::new(), request("user", "hi")).await.unwrap_err();
    assert_eq!((error.code, error.message.as_deref()), (ErrorCode::ResourceExhausted, Some("slow down")));
}
