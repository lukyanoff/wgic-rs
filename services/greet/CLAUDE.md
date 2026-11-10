# Box 3 — greet service

Says hello. **Reference shape for every service**: copy it when adding one.

- RPC: `greet.v1.GreetService/Greet` (`proto/greet/v1/greet.proto`)
- Rule: the name is trimmed, must be non-empty and ≤ 100 chars, else `invalid_argument`
- Config: none. Calls other services: none.
- Logs: `greeting sent` (name, transport). Metric: `greet.greetings{transport}` → Prometheus `greet_greetings_total`.
  Failures are logged and measured by the platform.

```
src/lib.rs            module(&Deps) -> Arc<dyn Module>   (composition root: kernel::module("greet", handler))
src/handler.rs        GreetHandler: the generated trait impl + a pure fn greeting(name) -> Result<String, ConnectError>
src/handler_tests.rs  unit tests for greeting()
tests/rpc.rs          integration: generated client over Deps::in_process (success + error code, logs/metrics checked)
```
E2E: `tests/e2e/tests/greet.rs` (HTTP + NATS).

- Test: `cargo test -p greet`, `cargo test -p e2e --test greet`.
