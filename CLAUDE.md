# connect-rust-example — Cognitive Boxes

Rust Connect/gRPC services that also run over NATS micro, plus a React playground (`web/`).
The goal of the layout is **low cognitive depth**: you can change one box without reading the others.

## Boxes (dependency arrows point down only)

| Box | Path | Owns | May depend on |
|---|---|---|---|
| 0 contracts | `proto/`, `crates/contracts` | `.proto` + generated code | — |
| 1 kernel | `crates/kernel` | `Module`, `Deps`, `Channel`, `Logger`, `Metrics`, `Transport`, `module()`, `stream_from()` | — |
| 2 platform | `crates/platform` | HTTP + NATS adapters, telemetry, host runner | kernel |
| 3 services | `services/<name>` | one service: `config.rs` + `handler.rs` (+ `drivers/` when pluggable) | contracts, kernel |
| 4 apps | `apps/host` | wiring (composition root) | anything |
| 5 e2e | `tests/e2e` | black-box tests against the real `host` binary | contracts, kernel, platform |

`scripts/check-boxes.sh` enforces these arrows. Each box has its own `CLAUDE.md`: read only the one for the box you work in.

## Rules

1. **Stay in one box per change.** A service change touches `services/<name>/` (and its proto / e2e file). An
   infrastructure change touches `crates/platform/` only. `apps/host` changes only to add a service line.
2. **Services never import `platform` or another service.** To call another service instance by id, use its
   generated client: `SensorServiceClient::new(deps.channel_to("screen-1"), deps.client_config())` (over NATS the
   id is the target host's `NATS_PREFIX`).
3. **Every service has 3 test layers**: unit (`src/handler_tests.rs`, pure helpers), integration
   (`services/<name>/tests/*.rs`, in-process via `Deps::in_process`), e2e (`tests/e2e/tests/<name>.rs`, HTTP **and** NATS).
4. **Write simple code** — use the `simple-code` skill for every code change. Readers are C#/Python/TS developers.
5. **Tests and code live in separate files.** `foo.rs` ends with
   `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` and the tests go in `foo_tests.rs` (next to it,
   starting with `use super::*;`). No inline `mod tests { ... }` (checked by `check-boxes.sh`).
6. **Logging and metrics are injected, never static.** A handler gets `deps.logger("svc")` and
   `deps.metrics("svc")` in its `new(deps)`; services must not depend on `tracing`/`opentelemetry`
   (checked). Platform already records every RPC's duration/status — services add only business metrics.
7. **Generated code is never edited by hand** (`crates/contracts/src/gen/`). Change `.proto`, run `npm run gen`.
8. **Only the services in `SERVICES` are built, and bad config stops the host.** `SERVICES` is required; each
   service reads its settings in `module(deps)` with `deps.setting(..)` / `deps.required(..)`, which panic with a
   clear message. No "not configured" fallbacks.

## Commands

```sh
podman compose up -d              # NATS, OTel collector, Jaeger :16686, Prometheus :9090, Grafana :3000
scripts/run-all.sh [--prod]       # build + run everything on NATS (--prod: release build, low memory per process)
scripts/verify.sh                 # definition of done: fmt, clippy, boxes, buf lint, all tests, web
cargo test -p greet               # one service: unit + integration (no network)
cargo test -p platform            # infrastructure only, no services compiled
cargo test -p e2e --test greet    # one service end-to-end
npm run gen                       # after any .proto change
SERVICES=greet cargo run -p host  # run one service (SERVICES is required: greet,sensor,llm_caller,visioner)
scripts/run-sensors.sh            # 4 `sensor` instances (screen-1/2, cam-1/2), NATS only, one process each
scripts/run-llm-callers.sh        # 4 llm_caller instances (llm-claude/agy/openai/openrouter), NATS only
scripts/run-visioners.sh          # vision-1: screen-1 + llm-claude,llm-agy in parallel, NATS only
curl -H 'Content-Type: application/json' -d '{"name":"Jane"}' localhost:8080/greet.v1.GreetService/Greet
```

## Definition of done

`scripts/verify.sh` is green. A Stop hook runs it automatically and blocks finishing while it fails
(it skips when nothing changed since the last green run). Don't weaken a check to make it pass —
fix the code, or ask the human.

## Add a service (checklist)

1. `proto/<name>/v1/<name>.proto` → `npm run gen` → add a `pub mod` block in `crates/contracts/src/lib.rs`.
2. Copy the shape of `services/greet/` (`lib.rs`, `handler.rs`, `handler_tests.rs`, `tests/rpc.rs`, `CLAUDE.md`,
   `Cargo.toml`); add `config.rs` when it has settings.
3. Add `<name>.workspace = true` to `apps/host/Cargo.toml` + root `[workspace.dependencies]`, and one line in
   `ALL_SERVICES` in `apps/host/src/main.rs`.
4. Add `tests/e2e/tests/<name>.rs` (HTTP + NATS).
5. `scripts/verify.sh`.
