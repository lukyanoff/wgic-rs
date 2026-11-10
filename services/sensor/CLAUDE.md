# Sensor Service Box Rules
- Pure domain crate: depends ONLY on `contracts`, `sdk`, and hardware capture crates (`xcap`, `nokhwa`, `image`).
- Inject `Arc<dyn Driver>` and `Arc<dyn IStateStore<SensorInstanceState>>` via `SensorHandler::new(...)`.
