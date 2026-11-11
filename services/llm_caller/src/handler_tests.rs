use super::*;
use crate::drivers::LlmOutput;
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
    let handler = LlmCallerHandler::new(Arc::new(MockDriver), state.clone());

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
}
