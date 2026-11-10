# Box 3 — screen_sensor service

Takes a picture of a screen and returns the encoded image + metadata. Works on Linux, macOS, Windows.

- RPCs: `screen_sensor.v1.ScreenSensorService/Capture` (`proto/screen_sensor/v1/screen_sensor.proto`)
- Metadata: size, format/mime, screen name/index/scale, driver, capture time + duration,
  `file_size_bytes` (encoded image), `base64_size_bytes` (its size in JSON/NATS replies)
- Config: the `ScreenSensorConfig` trait, injected into `ScreenSensor::new`. `module()` fills it from env:
  - `SCREEN_SENSOR_SCREEN` = `primary` (default) | `<index>` | `name:<NAME>`
  - `SCREEN_SENSOR_FORMAT` = `png` (default) | `jpeg` | `jpeg:<1..100>`
  - `SCREEN_SENSOR_DRIVER` = `auto` (default, = xcap) | `xcap` | `linux-x11`
  - An invalid value stops the host at startup.
- Drivers (sub-drivers per OS), all implementing `domain::CaptureDriver`:
  - `xcap`: the `xcap` crate. Windows GDI/DXGI, macOS CoreGraphics, Linux X11/Wayland (it picks the mechanism itself).
  - `linux-x11`: native X11 via `x11rb` (RandR monitors + `GetImage`). Linux only; on other OSes it returns `failed_precondition`.
- Errors → Connect code:
  - `ScreenNotFound` → `not_found`
  - `DriverNotSupported` → `failed_precondition`
  - `DriverUnavailable` (no display) → `unavailable`
  - capture/encode failures → `internal`
- Calls other services: none
- Logs: `screen captured` (driver, screen, format, bytes), `capture failed` (reason, error)
- Metrics: `screen_sensor.captures` {driver}, `screen_sensor.capture_ms` {driver}, `screen_sensor.failures` {reason, driver}
- Linux build needs (for xcap): `libclang-dev libxcb1-dev libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev libegl-dev libgbm-dev`

```
src/lib.rs           module(&Deps) (env config + drivers::create) and module_with(deps, config, driver)
src/config.rs        trait ScreenSensorConfig + SensorConfig (+ from_env) + parsers; config_tests.rs
src/domain.rs        trait CaptureDriver, RawFrame, CaptureError, pick_screen, ScreenSensor (capture → encode → metadata)
                     (no proto types; IO only behind CaptureDriver); domain_tests.rs with a FakeDriver
src/drivers/mod.rs   create(DriverKind) -> Arc<dyn CaptureDriver>   (factory; add one arm per new sub-driver)
src/drivers/xcap_driver.rs, linux_x11.rs (+ linux_x11_tests.rs), unsupported.rs (non-Linux stand-in)
src/handler.rs       generated trait impl: spawn_blocking(capture) → proto; CaptureError → ConnectError
tests/rpc.rs         integration over Deps::in_process with a fake driver (success, every error code, logs/metrics)
```
E2E: `tests/e2e/tests/screen_sensor.rs` (HTTP + NATS). It accepts a PNG, or `unavailable` when the host has no display.

Add a sub-driver: create `src/drivers/<os>_<mechanism>.rs` (`#[cfg(target_os = ...)]`), add a `DriverKind` case and
its parser name in `config.rs`, and add one arm in `drivers::create`.

- May depend on `contracts`, `kernel` and plain libraries. Never `platform`, another service,
  `anyhow`, or `tracing`/`opentelemetry`.
- Test: `cargo test -p screen_sensor` (unit + integration), `cargo test -p e2e --test screen_sensor`.
