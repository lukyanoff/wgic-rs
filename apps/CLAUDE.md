# Box 4 — apps (composition)

- `host/` — the server binary. `all_modules()` lists every service (one line each); `SERVICES=a,b`
  picks a subset. Keep `main.rs` boring: no logic here beyond wiring.
- To call services by hand use `curl` / `npx buf curl` (HTTP), `nats req` via nats-box (NATS) or the
  web playground — see HUMAN.md "Call it from the command line". No demo client app on purpose.

Run modes:
- one service, isolated: `SERVICES=greet cargo run -p host`
- everything: `cargo run -p host`
- N instances: start several with `NATS_URL=nats://127.0.0.1:4222 HTTP_DISABLED=1`; NATS load-balances.
