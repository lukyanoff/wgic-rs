# Box 3a — libs/sensor_base

The shared "base class" of image sensor services (`screen_sensor`, `camera_sensor`). It has no RPC of its own:
a service holds a `SensorRpc` and delegates its generated handler methods to it.

Rust has no inheritance, so the C# `abstract class SensorBase` is split in two:

| Piece | C# equivalent | Who writes it |
|---|---|---|
| `trait FrameSource` (`name`, `kind`, `list_devices`, `grab`) | the `abstract` members | each service, once per driver |
| `struct Sensor` (`capture`: grab → time → encode → metadata) | the base-class body | here |
| `struct SensorRpc` (`capture`, `list_devices`) | the shared handler code | here |

```
src/config.rs        trait SensorConfig + SensorSettings::from_env(deps, "<PREFIX>", default_id)
                     reads <PREFIX>_ID, <PREFIX>_DEVICE (default|<index>|name:<NAME>), <PREFIX>_FORMAT (png|jpeg[:q])
src/domain.rs        FrameSource, RawFrame, DeviceInfo, CaptureError, pick_device, rgb_to_rgba, Sensor
src/rpc.rs           SensorRpc: spawn_blocking, logs (`capture done`, `capture failed`), metrics, error codes, → sensor.v1 proto
src/unsupported.rs   UnsupportedSource: a driver for another OS (answers failed_precondition)
src/testing.rs       FakeSource: two fake devices LEFT/RIGHT, or a fixed error (for services' tests)
```

- Proto: `proto/sensor/v1/sensor.proto` (`ImageMetadata`, `DeviceInfo`); services import it into their own responses.
- Errors → Connect code:
  - `DeviceNotFound` → `not_found`
  - `DriverNotSupported` → `failed_precondition`
  - `DriverUnavailable` → `unavailable`
  - `DeviceBusy` → `resource_exhausted`
  - capture/encode → `internal`
- Logs: `capture done` (sensor_id, driver, device, format, bytes), `capture failed` (sensor_id, reason, error)
- Metrics (meter = service name): `<meter>.captures` {sensor_id, driver}, `<meter>.capture_ms` {sensor_id, driver},
  `<meter>.failures` {sensor_id, driver, reason}
- May depend on `contracts`, `kernel` and plain libraries. Never on `platform` or a service, `anyhow`,
  `tracing`/`opentelemetry`.
- Test: `cargo test -p sensor_base`. A change here affects every sensor service: also run `cargo test -p screen_sensor -p camera_sensor`.
