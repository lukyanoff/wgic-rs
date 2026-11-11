# Sensor Service Box Rules
- Pure domain crate: depends ONLY on `contracts`, `sdk`, and hardware capture crates (`xcap`, `nokhwa`, `image`).
- Inject `Arc<dyn Driver>`, `Arc<dyn IStateStore<SensorInstanceState>>` and `Arc<dyn ILogger>` via `SensorHandler::new(...)`.
- Screen and camera frames go through `drivers::encode_frame`: downscaled to `MAX_EDGE` (1568 px) and JPEG-encoded.
  A NATS message is capped at 1 MB, and a 4K PNG screenshot already exceeds it.
