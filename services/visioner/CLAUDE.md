# Visioner Service Box Rules
- Pure domain crate: depends ONLY on `contracts` and `sdk`.
- Inject all dependencies (`IStateStore`, `ClientRegistry`) via `VisionerHandler::new(...)`.
- Use `tracing::info!` / `#[tracing::instrument]` for observability.
- Unit tests in `src/*_tests.rs` must use `InMemoryStateStore` and `MapRegistry` mocks without NATS.
