use super::*;
use crate::drivers::LlmOutput;
use sdk::MemoryLogger;
use sdk::state::InMemoryStateStore;

struct MockDriver;
#[async_trait]
impl Driver for MockDriver {
    async fn call(&self, args: PromptArgs, _progress: ProgressSender) -> Result<LlmOutput, RpcError> {
        Ok(LlmOutput { text: format!("echo: {}", args.user_prompt), model: "mock-model".into() })
    }
}

#[tokio::test]
async fn llm_handler_calls_driver_and_updates_cas_state() {
    let state = Arc::new(InMemoryStateStore::new(LlmInstanceState::default()));
    let log = Arc::new(MemoryLogger::default());
    let handler = LlmCallerHandler::new(Arc::new(MockDriver), state.clone(), log.clone());

    let res = handler
        .call(
            LlmCallRequest {
                system_prompt: "sys".into(),
                text_prompt: "ctx".into(),
                user_prompt: "hello world".into(),
                images: vec![],
            },
            ProgressSender::noop(),
        )
        .await
        .unwrap();

    assert_eq!(res.text, "echo: hello world");
    assert_eq!(res.model, "mock-model");

    let saved = state.get().await.unwrap();
    assert_eq!(saved.total_calls, 1);
    assert_eq!(saved.last_model, "mock-model");

    let entries = log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "llm answered");
    assert_eq!(entries[0].fields, json!({ "model": "mock-model", "chars": 17 }));
}

struct FailingDriver;
#[async_trait]
impl Driver for FailingDriver {
    async fn call(&self, _args: PromptArgs, _progress: ProgressSender) -> Result<LlmOutput, RpcError> {
        Err(RpcError::unavailable("quota"))
    }
}

#[tokio::test]
async fn llm_handler_logs_a_driver_failure() {
    let state = Arc::new(InMemoryStateStore::new(LlmInstanceState::default()));
    let log = Arc::new(MemoryLogger::default());
    let handler = LlmCallerHandler::new(Arc::new(FailingDriver), state, log.clone());

    let request = LlmCallRequest { user_prompt: "hi".into(), ..Default::default() };
    let err = handler.call(request, ProgressSender::noop()).await.unwrap_err();

    assert_eq!(err.code, "unavailable");
    let entries = log.entries();
    assert_eq!(entries[0].level, sdk::Level::Warn);
    assert_eq!(entries[0].fields, json!({ "error.type": "unavailable", "error.message": "quota" }));
}
