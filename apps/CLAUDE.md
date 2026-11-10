# Box 4 — apps (composition)

- `host/` — the server binary. `all_modules()` lists every service (one line each); `SERVICES=a,b`
  picks a subset. Keep `main.rs` boring: no logic here beyond wiring.
- `cli/` — tiny client: `cargo run -p cli -- Jane` (HTTP to `ADDR`, or NATS when `NATS_URL` is set).

Run modes:
- one service, isolated: `SERVICES=greet cargo run -p host`
- everything: `cargo run -p host`
- N instances: start several with `NATS_URL=nats://127.0.0.1:4222 HTTP_DISABLED=1`; NATS load-balances.
