# Box 3 — llm_caller service

Sends a chat to an LLM and streams progress, then a JSON answer. One service "class"; each running
instance gets one injected driver and a fixed model / effort / temperature (from env, never from a request).

- RPC (`proto/llm_caller/v1/llm_caller.proto`), **server-streaming**:
  `llm_caller.v1.LlmCallerService/Call(CallRequest{messages[{role,content}], json_schema?})` →
  - `progress` messages, zero or more: stage `started|thinking|writing`, `partial_text` = all text so far,
    at most ~4/s plus every stage change
  - one `result`: `json` (validated, compact), `text` (raw), `metadata` (caller_id, driver, model, effort, temperature,
    tokens, duration, attributes such as `cost_usd`)
- Rust has no inheritance, so the "base classes" are a struct holding an interface:

  | Piece | C# equivalent |
  |---|---|
  | `domain::LlmDriver` (trait) | `interface ILlmDriver` |
  | `domain::LlmCaller` | the base-class body: validate the chat, add the JSON instruction, throttle progress, extract the JSON |
  | `drivers/cli/base.rs` `CliCaller` + `trait CliFlavor` | `abstract class CliCallerBase`: spawn, stdin, stream-json lines, kill on drop |
  | `drivers/cli/{claude,agy}.rs` | its subclasses: args + `parse_line` |
  | `drivers/http/base.rs` `HttpCaller` + `trait HttpFlavor` | `abstract class HttpCallerBase`: POST, SSE, status → error |
  | `drivers/http/{openai,openrouter}.rs` | its subclasses: body + `parse_event` |

- Config (env, read once at startup; a bad value stops the host). Full table in `HUMAN.md` → "LLM caller".
  - `LLM_CALLER_DRIVER` is `claude-cli|agy-cli|openai|openrouter`. When it is unset, `drivers/unconfigured.rs` answers
    every call with `failed_precondition`, so a host running other services still starts.
  - `LLM_CALLER_MODEL` (required) and `_EFFORT`. The CLIs take `low..max`; the HTTP drivers take `minimal..high`.
  - `_TEMPERATURE` is for the HTTP drivers only.
  - Also `_ID`, `_TIMEOUT_SECS`, `_BASE_URL` and `_CLI_PATH`.
  - API keys: `OPENAI_API_KEY` and `OPENROUTER_API_KEY`. A missing key fails the call, not the startup.
- agy is an agent (it ran shell commands and web searches unasked). It runs with `--sandbox --mode plan` in an empty temp
  folder, and the JSON instruction says "do not use tools". Its tool steps show up as `thinking`.
- With `json_schema`:
  - claude and agy get `--json-schema`; their `structured_output` wins over the text.
  - OpenAI and OpenRouter get a non-strict `json_schema` format.
  - Without a schema, the HTTP drivers use `json_object`.
- Cancel: when the client disconnects, the background task stops. The CLI child is killed (`kill_on_drop`) and the HTTP
  request is dropped.
- Errors → Connect codes:
  - bad chat or schema → `invalid_argument`
  - no driver, API key or CLI → `failed_precondition`
  - HTTP 401/403 → `unauthenticated`
  - HTTP 429 → `resource_exhausted`
  - HTTP 5xx, network error or timeout → `unavailable`
  - CLI or provider error → `internal`
  - an answer that isn't JSON → `data_loss`
- Logs: `llm call done`, `llm call failed`, `llm call cancelled`. Metrics (meter `llm_caller`): `calls`, `call_ms`,
  `output_tokens`, `failures` {caller_id, driver, model[, reason]}.

```
src/lib.rs               module(&Deps), module_with(deps, config, driver)
src/config.rs            LlmConfig (trait), LlmSettings::from_env / from_lookup, DriverKind; config_tests.rs
src/domain.rs            LlmDriver, LlmCaller, ProgressTracker, prepare, parse_role; domain_tests.rs
src/json.rs              json_instruction, extract_json; json_tests.rs
src/handler.rs           streaming RPC: background task → channel → stream; logs, metrics, error codes
src/drivers/mod.rs       create(deps, settings) -> Arc<dyn LlmDriver>
src/drivers/cli/         base.rs (CliCaller), claude.rs, agy.rs (+ *_tests.rs with recorded lines)
src/drivers/http/        base.rs (HttpCaller), openai.rs, openrouter.rs (+ *_tests.rs)
tests/rpc.rs             in-process stack with a fake driver: stream order, partial text, codes, metrics
tests/drivers.rs         CliCaller vs a shell script, HttpCaller vs a local SSE server
```
E2E: `tests/e2e/tests/llm_caller.rs`. The host runs the `openai` driver against a fake SSE server in the test, and
the test streams over HTTP **and** NATS.

- May depend on `contracts`, `kernel`, `libs/*` and plain libraries. Never `platform`, another service, `anyhow`,
  or `tracing`/`opentelemetry`.
- Test: `cargo test -p llm_caller`, `cargo test -p e2e --test llm_caller`. Real calls: `scripts/run-llm-callers.sh`.
