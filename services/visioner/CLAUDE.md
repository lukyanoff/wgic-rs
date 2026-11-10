# Box 3 — visioner service (orchestrator)

Takes a screenshot from one screen sensor instance, asks K llm_caller instances **in parallel** what they see,
streams everyone's progress, and returns every answer. The instances come from its configuration, not from
requests. It calls them over NATS by instance id, through the client factories in `libs/clients`.

- RPC (`proto/visioner/v1/visioner.proto`), **server-streaming**:
  `visioner.v1.VisionerService/Check(CheckRequest{question?})` →
  - `progress` messages: `source` = `screen` or the llm instance id. The screen stages are `capturing|captured`.
    The llm stages are `started|thinking|writing` (passed on from llm_caller with `partial_text`), then
    `done|failed|timeout`.
  - one `result`: `screen` info, one `answers[]` entry per model in config order (`ok`, `json`, or `error_code`).
- Fail safe:
  - one failed or timed-out model doesn't fail the check
  - no screenshot → error: `unavailable`, or `deadline_exceeded` on timeout
  - every model failed → `unavailable`, or `deadline_exceeded` when all timed out
  - not configured → `failed_precondition`
- Timeouts and cancel: each downstream call is wrapped in `tokio::time::timeout`, and its client carries the same
  deadline (`connect-timeout-ms`). On timeout the call is dropped here, and the callee's server ends the stream
  too (platform `DeadlinePolicy`), so llm_caller kills its CLI and drops its HTTP request. The e2e test proves it.
- Traces: the check runs **inside the response stream** (no `tokio::spawn`). The platform polls the stream inside
  this call's span, so the screen and llm calls are child spans in the same trace.
- Answer JSON (fixed `ANSWER_SCHEMA`): `{"summary": "...", "details": ["..."]}`.

| Piece | C# equivalent |
|---|---|
| `domain::Screen`, `domain::VisionLlm` | `interface IScreen`, `interface IVisionLlm` |
| `domain::Visioner` | the orchestrator class: screenshot → `Task.WhenAll(asks)` with per-call timeouts |
| `clients::ScreenClient`, `clients::LlmClient` | adapters around typed clients from a client factory |

- Config (env, read once):
  - `VISIONER_SCREEN`: the screen instance id, e.g. `screen-1`
  - `VISIONER_LLMS`: comma list, e.g. `llm-claude,llm-agy`
  - `VISIONER_ID` (default `vision`), `VISIONER_SCREEN_TIMEOUT_SECS` (15), `VISIONER_LLM_TIMEOUT_SECS` (120)
  - No `VISIONER_SCREEN` or no `NATS_URL` → every call answers `failed_precondition`, so a host running other
    services still starts.
- Logs: `check done` (answers_ok, answers, ms), `check failed` (reason), `llm failed` (caller_id, error).
  Metrics (meter `visioner`): `checks{result}`, `check_ms`, `llm_failures{caller_id, reason}`.

```
src/lib.rs          module(&Deps), module_with(deps, settings, screen, llms)
src/config.rs       VisionerSettings::from_env / from_lookup; config_tests.rs
src/domain.rs       Screen, VisionLlm, ProgressSink, Visioner::check, check_any_ok; domain_tests.rs (fakes)
src/clients.rs      ScreenClient, LlmClient over libs/clients factories; clients_tests.rs
src/handler.rs      streaming Check: work + progress channel in one stream; logs, metrics, codes; handler_tests.rs
tests/rpc.rs        real module + fake screen/llm services as in-process instances (Deps::in_process_instances)
```
E2E: `tests/e2e/tests/visioner.rs`. Four hosts over NATS (screen, an answering and a hanging fake-OpenAI
llm_caller, the visioner): one answer, one timeout, and the hanging call is cancelled.

- May depend on `contracts`, `kernel`, `libs/*` (`clients`) and plain libraries. Never `platform` or another service.
- Test: `cargo test -p visioner`, `cargo test -p e2e --test visioner`. Real run: `scripts/run-all.sh`.
