# Visioner Service Box Rules
- Pure domain crate: depends ONLY on `contracts` and `sdk`.
- Inject all dependencies via `VisionerHandler::new(cfg, state, VisionerClients { sensors, llms }, log)`.
- Timeouts are config (`sensor_timeout_secs` 15, `llm_timeout_secs` 120): `apps/host` builds the registries with
  them; the handler itself never times anything.
- Pass `progress.prefixed(<target>)` to each downstream call so the caller sees which source a step came from.
- Log through `self.log` (`ILogger`, `json!` fields); a failed downstream step logs `step failed` `{step, error.type}`.
  Spans: `#[tracing::instrument]`.
- Unit tests in `src/*_tests.rs` must use `InMemoryStateStore`, `MapRegistry` and `MemoryLogger` mocks without NATS.
