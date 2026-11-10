# Box 3 — visioner service (orchestrator)

Takes a screenshot from one sensor instance, asks several llm_caller instances **in parallel** what they see, streams
everyone's progress, and returns every answer. Which instances it calls comes from its settings, never from requests.

- RPC (`proto/visioner/v1/visioner.proto`), **server-streaming**:
  `visioner.v1.VisionerService/Check(CheckRequest{question?})` →
  - `progress`: `source` = `screen` or the llm instance id. Screen stages `capturing|captured`; llm stages
    `started|thinking|writing` (passed on from llm_caller), then `done|failed|timeout`.
  - one `result`: `screen` (the sensor's `ImageMetadata`), one `answers[]` entry per model in config order.
- The story is in `handler.rs`: capture → ask every model at once (`join_all` ≈ `Task.WhenAll`) → result.
  It calls the other instances with their **generated clients** over `deps.channel_to(id)`.
- Fail safe: one failed or timed-out model doesn't fail the check. No screenshot → `unavailable` (or
  `deadline_exceeded`); every model failed → `unavailable` (or `deadline_exceeded` when all timed out).
- Timeouts and cancel: each call is wrapped in `tokio::time::timeout` and its client carries the same deadline, so the
  called instance stops too (llm_caller kills its CLI / drops its HTTP request). The e2e test proves it.
- Traces: the check runs inside the response stream (no `tokio::spawn`), so downstream calls join the same trace.
- Config (env, read once; missing or bad stops the host):
  - `VISIONER_SCREEN` (required): the sensor instance id, e.g. `screen-1`
  - `VISIONER_LLMS` (required): comma list, e.g. `llm-claude,llm-agy`
  - `VISIONER_ID` (default `vision`), `VISIONER_SCREEN_TIMEOUT_SECS` (15), `VISIONER_LLM_TIMEOUT_SECS` (120)
- Logs: `llm failed` (caller_id, reason, error) for a model that failed inside a check that still succeeded.
  Metric: `visioner.llm_failures{caller_id, reason}`.

```
src/lib.rs          module(&Deps), module_with(deps, settings)
src/config.rs       VisionerSettings::from_env; config_tests.rs
src/handler.rs      the streaming Check, Reporter (progress), all_failed / no_screenshot / ask_request; handler_tests.rs
tests/rpc.rs        real module + fake sensor/llm services as in-process instances (Deps::in_process_instances)
```
E2E: `tests/e2e/tests/visioner.rs`: four hosts over NATS (sensor, an answering and a hanging fake-OpenAI llm_caller,
the visioner): one answer, one timeout, and the hanging call is cancelled.

- Test: `cargo test -p visioner`, `cargo test -p e2e --test visioner`. Real run: `scripts/run-all.sh`.
