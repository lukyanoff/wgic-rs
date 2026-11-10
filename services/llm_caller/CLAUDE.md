# Box 3 — llm_caller service

Sends a chat to an LLM, streams progress, then returns a JSON answer. One service; each running instance gets one
driver and a fixed model / effort from its settings, never from a request.

- RPC (`proto/llm_caller/v1/llm_caller.proto`), **server-streaming**:
  `llm_caller.v1.LlmCallerService/Call(CallRequest{messages[{role, content, images}], json_schema?})` →
  - `progress`, zero or more: stage `started|thinking|writing`, `partial_text` = all text so far,
    at most 4/s plus every stage change
  - one `result`: `json` (compact), `text` (raw), `metadata` (caller_id, driver, model, tokens, duration)
- The story is in `handler.rs`: check the chat → put the JSON instruction first → run the driver within 10 minutes,
  passing its output on as progress → extract the JSON.
- Driver hierarchy (the one place a "base class" earns its keep):

  | Piece | C# equivalent |
  |---|---|
  | `drivers::LlmDriver` (trait) | `interface ILlmDriver` |
  | `drivers/cli.rs` `CliDriver` + `trait CliFlavor` | `abstract class CliDriverBase`: spawn, stdin, read lines, kill on drop |
  | `drivers/claude.rs` `ClaudeCli`, `drivers/agy.rs` `AgyCli` | its subclasses: args + `parse_line` |
  | `drivers/http.rs` `HttpDriver` | one class for OpenAI and OpenRouter (both speak chat-completions SSE) |

- Config (env, read once; missing or bad stops the host):
  - `LLM_CALLER_DRIVER` (required): `claude-cli` | `agy-cli` | `openai` | `openrouter`
  - `LLM_CALLER_MODEL` (required), `_EFFORT` (default `medium`, passed through as-is), `_ID` (default `llm`)
  - `_BASE_URL` (HTTP drivers, for tests), `_CLI_PATH` (CLI drivers, for tests)
  - API keys `OPENAI_API_KEY` / `OPENROUTER_API_KEY`: a missing key fails the call (`failed_precondition`), not startup
- agy is an agent: it runs with `--sandbox --mode plan` in an empty temp folder; images become files `image-1.png`…
  in that folder. claude gets images as base64 blocks on stdin; the HTTP driver as `image_url` data URLs.
- `json_schema`: the CLIs get `--json-schema`; the HTTP driver a non-strict `json_schema` response format
  (`json_object` without one).
- Errors: bad chat or schema → `invalid_argument`; no CLI or API key → `failed_precondition`; HTTP 401/403 →
  `unauthenticated`; 429 → `resource_exhausted`; 5xx or network → `unavailable`; CLI or provider error → `internal`;
  no answer in 10 min → `deadline_exceeded`; an answer that isn't JSON → `data_loss`.
- Cancel: the call runs inside the response stream; when the client goes away the CLI child is killed and the HTTP
  request dropped.
- Logs: `llm call done` (caller_id, driver, model, duration, tokens). Metric: `llm_caller.output_tokens{driver, model}`.

```
src/lib.rs            module(&Deps), module_with(deps, settings, driver)
src/config.rs         LlmSettings::from_env
src/handler.rs        the streaming Call, Progress throttle, checked / with_json_instruction / extract_json; handler_tests.rs
src/drivers.rs        LlmDriver, Delta, LlmReply, create(deps, settings)
src/drivers/          cli.rs, claude.rs, agy.rs, http.rs (+ *_tests.rs with recorded lines / events)
tests/rpc.rs          in-process stack with a fake driver: stream order, partial text, codes, metrics
tests/drivers.rs      CliDriver vs shell scripts, HttpDriver vs a local SSE server
```
E2E: `tests/e2e/tests/llm_caller.rs`: the host runs the `openai` driver against a fake SSE server, over HTTP and NATS.

- Test: `cargo test -p llm_caller`, `cargo test -p e2e --test llm_caller`. Real calls: `scripts/run-llm-callers.sh`.
