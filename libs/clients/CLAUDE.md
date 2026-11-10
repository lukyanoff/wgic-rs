# Box 3a — libs/clients

Client factories: an instance id (from a service's config) → a ready generated client. ≈ C# typed client factory.

```rust
let client = ScreenSensorClients::new(&deps).get("screen-1", Duration::from_secs(15));
```

- `get(instance, timeout)` = `deps.channel_to(instance)` + `client_config().with_default_timeout(timeout)`.
  - Over NATS the instance id is the target host's `NATS_PREFIX`.
  - In tests, `Deps::in_process_instances` gives each id its own in-process router.
- The timeout is sent as `connect-timeout-ms`, and every server enforces it on streams too (platform `traced_service`).
  When it passes, the callee stops its work.
- One file per service, all the same shape:
  `greet.rs`, `screen_sensor.rs`, `camera_sensor.rs`, `llm_caller.rs`, `visioner.rs`.
  **New service → copy one file and add it to `lib.rs`.**
- May depend on `contracts` and `kernel` only.
- Test: `cargo test -p clients` (two fake greet instances; a call reaches the instance it names).
