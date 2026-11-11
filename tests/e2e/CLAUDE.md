# Box 5 — e2e

Black-box-ish tests over a real NATS (`podman compose up -d`): handlers are mounted with `sdk::serve_endpoint` and
called with the sdk's NATS clients, exactly as in production.

- Harness: `src/lib.rs` → `TestEnv::connect()`, `env.instance_name("x")` (unique per run, so tests run in parallel
  and never hit dev instances), `env.mount_endpoint(..)`, `env.sensor_client / llm_client / visioner_client`.
  `src/fake_openai.rs` streams SSE like the real chat-completions API.
- One file per service: `tests/<service>.rs`.
- Cover: happy path + at least one error code across NATS.
- Run: `cargo test -p e2e` or `cargo test -p e2e --test <service>`.
