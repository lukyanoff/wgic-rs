# Box 2 — platform (infrastructure)

Exposes `kernel::Module`s to the world and wires telemetry. **Knows nothing about any domain or
proto** — if you need a service name or message type here, the design is wrong.

```
src/lib.rs          entry points: init_telemetry, run, select_modules, deps_from_env, traced_service
src/host.rs         run modules on adapters chosen by env (ADDR, HTTP_DISABLED, NATS_URL), SERVICES selection
src/adapter.rs      trait Adapter + HttpAdapter (all modules, one port) + NatsAdapter (one micro service per module)
src/http_server.rs  axum server + CORS (CORS_ORIGINS)
src/nats/           wire.rs (pure naming/status rules) · server.rs (micro endpoints) · client.rs (NatsTransport)
src/telemetry/      init.rs · logs.rs (JSON) · propagation.rs (W3C headers) · interceptor.rs (rpc.server) · client.rs (rpc.client)
```

- May depend on `kernel` only. Errors: `anyhow::Result` + `.context(...)`.
- New transport (e.g. JetStream)? Implement `Adapter` in a new file; register it in `host.rs::adapters_from_env`.
- Keep wire-format logic as pure functions in `nats/wire.rs` so it is unit-testable.
- Test: `cargo test -p platform` (unit). End-to-end behaviour is covered by `tests/e2e`.
- NATS wire format and env vars are documented in `HUMAN.md` — update it when you change them.
