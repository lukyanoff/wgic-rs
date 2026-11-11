# LLM Caller Service Box Rules
- Pure domain crate: depends ONLY on `contracts`, `sdk`, `reqwest`, `futures`, and `tempfile`.
- Inject `Arc<dyn Driver>` and `Arc<dyn IStateStore<LlmInstanceState>>` via `LlmCallerHandler::new(...)`.
- Every driver streams: `progress.report("thinking" | "writing")` once per stage change and `progress.text(delta)` for
  answer text. CLI drivers parse `--output-format stream-json` line by line (`cli::run_streaming` + a `parse_line`
  per driver); `http` parses SSE (`SseText`).
- CLI processes run with `kill_on_drop(true)` in a `tempfile` work folder (`cli::WorkDir`), so a timed-out call
  leaves no process or file behind.
- Config `model` is passed to the CLI (`--model`) only when set; empty = the CLI's default.
