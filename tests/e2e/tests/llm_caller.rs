use contracts::{ILlmCallerService, ImageInput, LlmCallRequest, LlmCallerEndpoint};
use e2e::TestEnv;
use e2e::fake_openai::FakeOpenAi;
use llm_caller::{Config, DriverKind, LlmCallerHandler, LlmInstanceState, build_driver};
use sdk::{MemoryLogger, ProgressSender};
use std::sync::Arc;

#[tokio::test]
async fn llm_caller_invokes_http_driver_and_updates_cas_state() -> anyhow::Result<()> {
    let fake_server = FakeOpenAi::start().await;
    let mut env = TestEnv::connect().await?;
    let inst = env.instance_name("openai");

    let cfg = Config {
        kind: DriverKind::Http,
        model: "gpt-4o-mini-test".into(),
        endpoint: fake_server.url(),
        api_key: "test-key".into(),
        ..Default::default()
    };

    let state = env.state_store::<LlmInstanceState>("llm_caller", &inst);
    let driver = build_driver(cfg)?;
    let handler = Arc::new(LlmCallerHandler::new(driver, state.clone(), Arc::new(MemoryLogger::default())));

    env.mount_endpoint("llm_caller", &inst, Arc::new(LlmCallerEndpoint(handler))).await?;

    let client = env.llm_client(&inst);
    let resp = client
        .call(
            LlmCallRequest {
                system_prompt: "You are helpful".into(),
                text_prompt: "Context".into(),
                user_prompt: "What is in this picture?".into(),
                images: vec![ImageInput { data: vec![1, 2, 3, 4], mime_type: "image/png".into() }],
            },
            ProgressSender::noop(),
        )
        .await?;

    assert!(resp.text.contains("What is in this picture?"));
    assert_eq!(resp.model, "gpt-4o-mini-test");

    let saved = state.get().await?;
    assert_eq!(saved.total_calls, 1);
    assert_eq!(saved.last_model, "gpt-4o-mini-test");
    Ok(())
}

#[tokio::test]
async fn error_codes_cross_nats_as_micro_error_replies() -> anyhow::Result<()> {
    let mut env = TestEnv::connect().await?;
    let inst = env.instance_name("errors");
    let cfg = Config { kind: DriverKind::Http, endpoint: "http://127.0.0.1:9/never".into(), ..Default::default() };
    let state = env.state_store::<LlmInstanceState>("llm_caller", &inst);
    let handler = Arc::new(LlmCallerHandler::new(build_driver(cfg)?, state, Arc::new(MemoryLogger::default())));
    env.mount_endpoint("llm_caller", &inst, Arc::new(LlmCallerEndpoint(handler))).await?;

    // Handler error → `Nats-Service-Error` header → the same code on the client.
    let empty = LlmCallRequest { user_prompt: "  ".into(), ..Default::default() };
    let err = env.llm_client(&inst).call(empty, ProgressSender::noop()).await.unwrap_err();
    assert_eq!(err.code, "invalid_argument");

    // Nobody serves this instance → NATS "no responders" → `unavailable`, without waiting for the timeout.
    let err = env.llm_client("nobody-here").call(LlmCallRequest::default(), ProgressSender::noop()).await.unwrap_err();
    assert_eq!(err.code, "unavailable");
    Ok(())
}
