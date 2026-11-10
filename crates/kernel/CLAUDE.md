# Box 1 — kernel

The whole contract between services and infrastructure. Small on purpose: every line here is
something all ~10 services must understand. Add to it rarely.

| Item | What it is |
|---|---|
| `Module` (trait) | a service plugs into the host: `name()` + `register(router)` |
| `Deps` | what the host gives a service: `channel()`, `client_config()`, `logger(cat)`, `metrics(meter)`, `env(key)`; `Deps::in_process(&[modules])` + `.with_logs(..)`/`.with_metrics(..)` for tests |
| `Channel` | one client transport type over HTTP / NATS / in-process, so services use `XClient<Channel>` |
| `ChannelFactory` (trait), `deps.channel_to(id)` | a channel to one named instance (over NATS: the host with `NATS_PREFIX=id`); `Deps::in_process_instances(vec![(id, modules)])` for tests. Services use it through `libs/clients` |
| `Logger` | injected logger (≈ `ILogger<T>`): `info/warn/error/debug(msg, [kv(..)])`, `with([...])` ≈ `BeginScope` |
| `Metrics` | injected metrics (≈ `IMetrics`): `count/record/set(name, value, tags)`, name prefixed with the meter |
| `kv`, `Field`, `FieldValue` | structured key/value for log fields and metric tags |
| `LogBackend`, `MetricsBackend` | implemented by platform and `testing`; services never use them |
| `testing::{MemoryLogs, MemoryMetrics}` | capture logs/metrics in tests; `NoopLogs`/`NoopMetrics` are the `Deps` defaults |
| `Transport`, `RequestContextExt` | `ctx.transport()` → how a request arrived |

- Must not depend on any workspace crate (no `platform`, no `contracts`).
- No IO, no env parsing beyond `Deps::env`, no telemetry, no `anyhow` (services use their own error enums).
- Test: `cargo test -p kernel`.
