# Box 3 — camera_sensor service

Takes a picture from a webcam and returns the encoded image + metadata. Works on Linux, macOS, Windows.
Built on `libs/sensor_base` (config, encoding, metadata, logs/metrics, error codes); this crate only adds camera drivers.

- RPCs (`proto/camera_sensor/v1/camera_sensor.proto`):
  - `camera_sensor.v1.CameraSensorService/Capture` → image + `sensor.v1.ImageMetadata`
  - `camera_sensor.v1.CameraSensorService/ListDevices` → cameras (index, name, is_default = first camera)
- Config (env), read at startup; an invalid value stops the host:
  - `CAMERA_SENSOR_ID` (default `camera`): the instance id, put in metadata, logs and metrics
  - `CAMERA_SENSOR_DEVICE` = `default` (first camera) | `<index>` | `name:<NAME>`
  - `CAMERA_SENSOR_FORMAT` = `png` (default) | `jpeg` | `jpeg:<1..100>`
  - `CAMERA_SENSOR_DRIVER` = `auto` (default) | `nokhwa-v4l` | `nokhwa-avfoundation` | `nokhwa-msmf`
  - `CAMERA_SENSOR_RESOLUTION` = `max` (default) | `<W>x<H>` (camera must offer exactly that size)
  - `CAMERA_SENSOR_WARMUP_FRAMES` = `0..100` (default 5): frames dropped first so auto-exposure settles
- Drivers: one `NokhwaDriver` (nokhwa crate), one backend per OS camera API. A backend for another OS answers
  `failed_precondition`.
  - Each capture opens the camera, drops the warm-up frames, keeps one, and closes it. So the camera light goes off and other apps
    can use the camera between calls; each call takes roughly 1–2 s.
  - Linux: `/dev/video*` metadata nodes (2 per UVC webcam) are hidden by asking V4L2 for `VIDEO_CAPTURE`. Cameras are sorted
    by device number, so indices stay stable.
  - macOS: `module()` calls `nokhwa_initialize` (the camera permission prompt). This path is not tested yet.
- Metadata attributes: `native_format` (e.g. `MJPEG`), `fps`.
- Errors: as in `libs/sensor_base/CLAUDE.md`. A camera opened by another app → `resource_exhausted`. Meter `camera_sensor`.

```
src/lib.rs                       module(&Deps) and module_with(deps, config, source)
src/config.rs                    CameraOptions (driver, resolution, warm-up) from env; config_tests.rs
src/drivers/mod.rs               create(CameraOptions) -> Arc<dyn FrameSource>
src/drivers/nokhwa_driver.rs     list/open/warm up/grab/close; nokhwa_driver_tests.rs (error mapping, resolution)
src/handler.rs                   generated trait impl: delegates to sensor_base::SensorRpc
tests/rpc.rs                     integration over Deps::in_process with sensor_base::testing::FakeSource
```
E2E: `tests/e2e/tests/camera_sensor.rs` (HTTP + NATS). It accepts a PNG, or `unavailable`/`not_found`/`resource_exhausted` when no camera is usable.

- May depend on `contracts`, `kernel`, `libs/*` and plain libraries. Never `platform`, another service, `anyhow`,
  or `tracing`/`opentelemetry`.
- Test: `cargo test -p camera_sensor`, `cargo test -p e2e --test camera_sensor`.
