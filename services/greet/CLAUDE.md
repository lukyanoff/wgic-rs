# Box 3 — greet service

Says hello. **Reference shape for every service** — copy it when adding one.

- RPCs: `greet.v1.GreetService/Greet` (`proto/greet/v1/greet.proto`)
- Rules: name is trimmed, must be non-empty and ≤ 100 chars → else `invalid_argument`
- Calls other services: none
- Config (env): none
- Logs: `greeting sent` (name, transport), `greeting rejected` (reason)
- Metrics: `greet.greetings` {transport}, `greet.rejected` {reason} → Prometheus `greet_greetings_total`, `greet_rejected_total`

```
src/lib.rs      module(&Deps) -> Arc<dyn Module>   (the only public item)
src/domain.rs        pure rules                     (no async, no IO, no proto types)
src/domain_tests.rs  unit tests for domain.rs
src/handler.rs  generated trait impl: proto -> domain -> proto, domain error -> ConnectError;
                Logger + Metrics injected in GreetHandler::new(deps)
tests/rpc.rs    integration: generated client over Deps::in_process (success + every error code,
                logs/metrics checked with MemoryLogs/MemoryMetrics)
```
E2E: `tests/e2e/tests/greet.rs` (HTTP + NATS).

- May depend on `contracts`, `kernel` and plain libraries. Never `platform`, another service,
  `anyhow`, or `tracing`/`opentelemetry` (log and measure through the injected `Logger`/`Metrics`).
- Errors: `GreetError` (domain enum) → `to_connect_error` in `handler.rs` → Connect code.
- Test: `cargo test -p greet` (unit + integration), `cargo test -p e2e --test greet`.
