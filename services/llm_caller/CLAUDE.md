# LLM Caller Service Box Rules
- Pure domain crate: depends ONLY on `contracts`, `sdk`, `reqwest`, and `tempfile`.
- Inject `Arc<dyn Driver>` and `Arc<dyn IStateStore<LlmInstanceState>>` via `LlmCallerHandler::new(...)`.
- Always use `tempfile::Builder` RAII guards for CLI image staging.
