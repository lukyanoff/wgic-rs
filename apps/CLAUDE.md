# Box 4 — apps (composition)

- `host/` — the server binary. `ALL_SERVICES` lists every service as `("name", name::module)`; `SERVICES=a,b`
  (required) picks which ones are built. Keep `main.rs` boring: no logic here beyond wiring.
- To call services by hand use `curl` / `npx buf curl` (HTTP), `nats req` via nats-box (NATS) or the
  web playground — see HUMAN.md "Call it from the command line". No demo client app on purpose.

Run modes:
- one service, isolated: `SERVICES=greet cargo run -p host`
- everything: `SERVICES=greet,sensor,llm_caller,visioner cargo run -p host` (each needs its settings, see HUMAN.md)
- N instances: start several with `NATS_URL=nats://127.0.0.1:4222 HTTP_DISABLED=1`; NATS load-balances.
- one process per device (different config each): give every process its own `ADDR`, `NATS_PREFIX` and `NATS_INSTANCE`,
  or NATS load-balances calls across them (`NATS_INSTANCE` also gives each its own `nats micro ls` row). `scripts/run-sensors.sh` does this for 2 screens + 2 cameras (`SERVICES=sensor`).
