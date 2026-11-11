# Visioner Service Box Rules
- Pure domain crate: depends ONLY on `contracts` and `sdk`.
- Inject all dependencies (`IStateStore`, `ClientRegistry`) via `VisionerHandler::new(...)`.
- Timeouts are config (`sensor_timeout_secs` 15, `llm_timeout_secs` 120): `apps/host` builds the registries with
  them; the handler itself never times anything.
- Pass `progress.prefixed(<target>)` to each downstream call so the caller sees which source a step came from.
- Use `tracing::info!` / `#[tracing::instrument]` for observability.
- Unit tests in `src/*_tests.rs` must use `InMemoryStateStore` and `MapRegistry` mocks without NATS.
