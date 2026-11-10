# Box 3 — sensor service

Takes a picture of a screen or from a camera and returns the image + metadata. One service; each running instance
is one device, chosen by its settings (`scripts/run-sensors.sh` starts screen-1/2 and cam-1/2).

- RPCs (`proto/sensor/v1/sensor.proto`):
  - `Capture` → `image` (PNG or JPEG bytes) + `metadata` (sensor_id, kind, width, height, mime_type, device, times)
  - `ListDevices` → the screens or cameras this machine has (`index`, `name`, `is_default`)
- Config (env, read once; a bad value stops the host):
  - `SENSOR_KIND` = `screen` (default) | `camera`
  - `SENSOR_ID` (default: the kind), `SENSOR_DEVICE` = an index (default: the default device)
  - `SENSOR_FORMAT` = `png` (default) | `jpeg` (quality 85)
- Errors: no such device → `not_found`; no display / camera → `unavailable`; camera busy → `resource_exhausted`;
  anything else → `internal`.
- Logs / metrics: none of its own (the platform records every RPC; the metadata says the rest).

| Piece | C# equivalent |
|---|---|
| `drivers::FrameSource` (trait) | `interface IFrameSource` |
| `drivers/screen.rs` `Screen` (xcap), `drivers/camera.rs` `Camera` (nokhwa) | its two implementations |
| `drivers/fake.rs` `FakeSource` | a test double (used by `tests/rpc.rs`) |
| `tokio::task::spawn_blocking` in `handler.rs` | `Task.Run` for the blocking grab |

```
src/lib.rs          module(&Deps), module_with(settings, source)
src/config.rs       SensorSettings::from_env, ImageFormat, parse_device / parse_format; config_tests.rs
src/drivers.rs      FrameSource, RawFrame, by_kind, pick_device; drivers_tests.rs
src/drivers/        screen.rs, camera.rs (+ camera_tests.rs), fake.rs
src/handler.rs      Capture: grab → encode → ImageMetadata; ListDevices; handler_tests.rs
tests/rpc.rs        generated client over Deps::in_process with FakeSource
```
E2E: `tests/e2e/tests/sensor.rs` (screen over HTTP and NATS; camera accepts "no camera" codes).

- Test: `cargo test -p sensor`, `cargo test -p e2e --test sensor`. Real devices: `scripts/run-sensors.sh`.
