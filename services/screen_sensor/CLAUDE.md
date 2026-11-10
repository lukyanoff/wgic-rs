# Box 3 — screen_sensor service

Takes a picture of a screen and returns the encoded image + metadata. Works on Linux, macOS, Windows.
Built on `libs/sensor_base` (config, encoding, metadata, logs/metrics, error codes); this crate only adds screen drivers.

- RPCs (`proto/screen_sensor/v1/screen_sensor.proto`):
  - `screen_sensor.v1.ScreenSensorService/Capture` → image + `sensor.v1.ImageMetadata`
  - `screen_sensor.v1.ScreenSensorService/ListDevices` → monitors (index, name, is_default)
- Config (env), read at startup; an invalid value stops the host:
  - `SCREEN_SENSOR_ID` (default `screen`): the instance id, put in metadata, logs and metrics
  - `SCREEN_SENSOR_DEVICE` = `default` (primary monitor) | `<index>` | `name:<NAME>`
  - `SCREEN_SENSOR_FORMAT` = `png` (default) | `jpeg` | `jpeg:<1..100>`
  - `SCREEN_SENSOR_DRIVER` = `auto` (default, = xcap) | `xcap` | `linux-x11`
- Drivers (sub-drivers), each implementing `sensor_base::FrameSource` with kind `screen`:
  - `xcap`: Windows GDI/DXGI, macOS CoreGraphics, Linux X11/Wayland (it picks the mechanism itself)
  - `linux-x11`: native X11 via `x11rb` (RandR monitors + `GetImage`). Linux only; elsewhere it answers `failed_precondition`.
- Metadata attribute: `scale_factor`.
- Errors, logs and metrics: see `libs/sensor_base/CLAUDE.md` (meter `screen_sensor`).
- Linux build needs (for xcap): `libclang-dev libxcb1-dev libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev libegl-dev libgbm-dev`

```
src/lib.rs           module(&Deps) (SensorSettings + driver from env) and module_with(deps, config, source)
src/config.rs        SCREEN_SENSOR_DRIVER → DriverKind; config_tests.rs
src/drivers/mod.rs   create(DriverKind) -> Arc<dyn FrameSource>   (factory; add one arm per new sub-driver)
src/drivers/xcap_driver.rs, linux_x11.rs (+ linux_x11_tests.rs)
src/handler.rs       generated trait impl: delegates to sensor_base::SensorRpc, wraps into screen_sensor.v1 messages
tests/rpc.rs         integration over Deps::in_process with sensor_base::testing::FakeSource
```
E2E: `tests/e2e/tests/screen_sensor.rs` (HTTP + NATS). It accepts a PNG, or `unavailable` when the host has no display.

To add a sub-driver: add `src/drivers/<os>_<mechanism>.rs` (`#[cfg(target_os = ...)]`), add a `DriverKind` case and
its name in `config.rs`, and add one arm in `drivers::create`.

- May depend on `contracts`, `kernel`, `libs/*` and plain libraries. Never `platform`, another service, `anyhow`,
  or `tracing`/`opentelemetry`.
- Test: `cargo test -p screen_sensor` (unit + integration), `cargo test -p e2e --test screen_sensor`.
