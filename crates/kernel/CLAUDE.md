# Box 1 — kernel

The whole contract between services and infrastructure. Small on purpose: every line here is
something all ~10 services must understand. Add to it rarely.

| Item | What it is |
|---|---|
| `Module` (trait), `module(name, handler)` | a service plugs into the host; `kernel::module("greet", handler)` wraps any generated-service handler. `ModuleFactory` = `fn(&Deps) -> Arc<dyn Module>` |
| `stream_from(\|tx\| async { .. })`, `StreamTx` | a server stream as a plain async function: `tx.send(msg)`, `tx.fail(err)`; runs inside the stream, so a disconnect cancels it |
| `Deps` | what the host gives a service: `channel()`, `client_config()`, `logger(cat)`, `metrics(meter)`, `env(key)`, `setting(key, default, expected, parse)`, `required(key, expected)` (bad/missing → startup panic); `Deps::in_process(&[modules])` + `.with_logs(..)`/`.with_metrics(..)`/`.with_env(..)` for tests |
| `Channel` | one client transport type over HTTP / NATS / in-process, so services use `XClient<Channel>` |
| `ChannelFactory` (trait), `deps.channel_to(id)` | a channel to one named instance (over NATS: the host with `NATS_PREFIX=id`); `Deps::in_process_instances(vec![(id, modules)])` for tests. Services pass it to a generated client: `XClient::new(deps.channel_to(id), deps.client_config())` |
| `Logger` | injected logger (≈ `ILogger<T>`): `info/warn/error/debug(msg, [kv(..)])`, `with([...])` ≈ `BeginScope` |
| `Metrics` | injected metrics (≈ `IMetrics`): `count/record/set(name, value, tags)`, name prefixed with the meter |
| `kv`, `Field`, `FieldValue` | structured key/value for log fields and metric tags |
| `LogBackend`, `MetricsBackend` | implemented by platform and `testing`; services never use them |
| `testing::{MemoryLogs, MemoryMetrics}` | capture logs/metrics in tests; `NoopLogs`/`NoopMetrics` are the `Deps` defaults |
| `Transport`, `RequestContextExt` | `ctx.transport()` → how a request arrived |

- Must not depend on any workspace crate (no `platform`, no `contracts`).
- No IO, no env parsing beyond `Deps::env`, no telemetry, no `anyhow` (services return `ConnectError`).
- Test: `cargo test -p kernel`.
