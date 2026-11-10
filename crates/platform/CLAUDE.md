# Box 2 — platform (infrastructure)

Exposes `kernel::Module`s to the world and wires telemetry. **Knows nothing about any domain or
proto** — if you need a service name or message type here, the design is wrong.

```
src/lib.rs          entry points: init_telemetry, run, build_modules, deps_from_env, traced_service
src/host.rs         run modules on adapters chosen by env (ADDR, HTTP_DISABLED, NATS_URL), build_modules: build only the services in SERVICES
src/adapter.rs      trait Adapter + HttpAdapter (all modules, one port) + NatsAdapter (one micro service per module)
src/http_server.rs  axum server + CORS (CORS_ORIGINS)
src/nats/           wire.rs (pure naming/status rules) · server.rs (micro endpoints) · client.rs (NatsTransport)
                    stream.rs (server streaming: body chunks → reply inbox, then `Nats-Stream-End`)
src/telemetry/      init.rs (traces/logs/metrics providers) · logs.rs (JSON) · propagation.rs (W3C headers)
                    interceptor.rs (rpc.server span + rpc.server.call.duration) · client.rs (rpc.client)
                    log_backend.rs (TracingLogs) · metrics_backend.rs (OtelMetrics) · test_meter.rs (tests only)
```

- `host.rs::NatsChannels` implements `kernel::ChannelFactory`: `deps.channel_to("screen-1")` → NATS prefix `screen-1`.
- `traced_service` enforces the client's deadline on streams too (`DeadlinePolicy::with_enforce_on_streams`): when
  `connect-timeout-ms` passes, the stream ends with `deadline_exceeded` and the handler's stream is dropped (cancel).
- May depend on `kernel` only. Errors: `anyhow::Result` + `.context(...)`.
- New transport (e.g. JetStream)? Implement `Adapter` in a new file; register it in `host.rs::adapters_from_env`.
- Keep wire-format logic as pure functions in `nats/wire.rs` so it is unit-testable.
- Service logs go through `TracingLogs` with target `service`; their fields travel as JSON in
  `log.fields` (flattened by `logs.rs` and by the collector). Keep both in sync.
- Test: `cargo test -p platform` (unit; OTel metrics via `test_meter::TestMeter`). End-to-end behaviour is covered by `tests/e2e`.
- NATS wire format and env vars are documented in `HUMAN.md` — update it when you change them.
