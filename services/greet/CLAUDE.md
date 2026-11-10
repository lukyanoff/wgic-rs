# Box 3 — greet service

Says hello. **Reference shape for every service** — copy it when adding one.

- RPCs: `greet.v1.GreetService/Greet` (`proto/greet/v1/greet.proto`)
- Rules: name is trimmed, must be non-empty and ≤ 100 chars → else `invalid_argument`
- Calls other services: none
- Config (env): none

```
src/lib.rs      module(&Deps) -> Arc<dyn Module>   (the only public item)
src/domain.rs        pure rules                     (no async, no IO, no proto types)
src/domain_tests.rs  unit tests for domain.rs
src/handler.rs  generated trait impl: proto -> domain -> proto, domain error -> ConnectError
tests/rpc.rs    integration: generated client over Deps::in_process (success + every error code)
```
E2E: `tests/e2e/tests/greet.rs` (HTTP + NATS).

- May depend on `contracts`, `kernel` and plain libraries. Never `platform`, another service, or `anyhow`.
- Errors: `GreetError` (domain enum) → `to_connect_error` in `handler.rs` → Connect code.
- Test: `cargo test -p greet` (unit + integration), `cargo test -p e2e --test greet`.
