# Box 4 — apps (composition)

- `host/` — the only binary: `host --module <sensor|llm_caller|visioner> --instance <id> --config '<json>'`.
  It initialises telemetry, builds the module's drivers / registries / KV state store, and serves the endpoint as a
  NATS micro service. Keep `main.rs` boring: no logic here beyond wiring.
- Replicas: start the same `--module --instance` twice; NATS load-balances (queue group) and they share KV state.
- `scripts/run-all.sh` starts the dev mesh with OTLP export on. Call it from the web console or
  `podman compose run --rm nats-box nats micro …` (see HUMAN.md).
