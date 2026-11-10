# Box 1 — kernel

The whole contract between services and infrastructure. Small on purpose: every line here is
something all ~10 services must understand. Add to it rarely.

| Item | What it is |
|---|---|
| `Module` (trait) | a service plugs into the host: `name()` + `register(router)` |
| `Deps` | what the host gives a service: `channel()`, `client_config()`, `env(key)`; `Deps::in_process(&[modules])` for tests |
| `Channel` | one client transport type over HTTP / NATS / in-process, so services use `XClient<Channel>` |
| `Transport`, `RequestContextExt` | `ctx.transport()` → how a request arrived |

- Must not depend on any workspace crate (no `platform`, no `contracts`).
- No IO, no env parsing beyond `Deps::env`, no telemetry, no `anyhow` (services use their own error enums).
- Test: `cargo test -p kernel`.
