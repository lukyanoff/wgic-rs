# WGIC: a NATS service mesh in Rust

Three small services, each running as its own process:

- **sensor** captures an image.
- **llm_caller** asks an LLM about it.
- **visioner** chains the two.

Every process is a NATS micro service. The repo also includes a web console, OpenTelemetry tracing (Jaeger) and metrics (Prometheus/Grafana).
The code layout and its rules are in `CLAUDE.md`.

## Requirements

- Rust (edition 2024)
- Node 20+
- podman + podman-compose
- The `agy` and/or `claude` CLIs on `PATH`, for the LLM callers

## Run it

```bash
podman compose up -d          # NATS (4222, ws 9222), otel-collector, Jaeger, Prometheus, Grafana
scripts/run-all.sh            # sensors, LLM callers, 2 visioner replicas; Ctrl+C stops them all
cd web && npm install && npm run dev   # console: http://localhost:5173
```

After changing a `.proto`, run `npm run gen` in the repo root. It rebuilds `web/public/descriptors.binpb` for the console.

One process is `host --module <module> --instance <id> --config '<json>'`:

| Module | Config (JSON) |
|---|---|
| `sensor` | `{"kind":"screen","index":0}`, `{"kind":"camera","index":2}` (N in `/dev/videoN`), `{"kind":"fake"}` |
| `llm_caller` | `{"kind":"agy"}`, `{"kind":"claude","model":"claude-haiku-5-5"}`, `{"kind":"http","endpoint":"…","api_key":"…","model":"…"}` |
| `visioner` | `{"sensor":"screen","llm_caller":"agy","user_prompt":"…","sensor_timeout_secs":15,"llm_timeout_secs":120}` |

To add a replica, start the same `--module --instance` again. NATS load-balances between them (queue group `q.<module>.<instance>`), and they share state in the KV bucket `wgic_instance_state`.

## Web console

1. Pick a method and an instance. The instance chips are whatever is running right now.
2. Edit the JSON and set a timeout.
3. Invoke.

While the call runs you see:
- **Live progress:** steps with elapsed time, labelled by source, e.g. `+2.3s [screen] captured 170719 bytes` and `[claude] thinking`.
- **The answer as it is written:** then the final response, or the error code and message.
- **Instance state:** the CAS state of every instance (KV).
- **The trace:** the call's spans from Jaeger (browser → visioner → sensor / llm_caller), with a link to Jaeger.
- **Running services:** `nats micro ls` + `stats` (replicas, calls, errors, average time, last error). Click a row to target that instance.

## Timeouts

- **The caller's budget:** every call carries it in the `Wgic-Timeout-Ms` header. The console's timeout field and the visioner's `sensor_timeout_secs` / `llm_timeout_secs` set it.
- **Downstream calls:** a server passes on only the time it has left. When the budget runs out, every hop stops its work and the caller gets `deadline_exceeded`. A running `claude`/`agy` process is killed, and its temp folder is deleted.
- **Upper limit:** no call runs longer than 600 s (`sdk::MAX_CALL`).
- **Missing instance:** calling an instance that isn't running fails at once with `unavailable: nothing is listening on …`.

## NATS micro commands

Run them through the nats-box container: `podman compose run --rm nats-box <command>`.

```bash
nats micro ls                  # services (sensor, llm_caller, visioner) and every running process
nats micro info visioner       # endpoints, subjects, queue groups, metadata (instance), stats
nats micro stats llm_caller    # requests / errors / processing time per process
nats micro ping                # every process answers with its RTT
nats sub 'rpc.>'               # watch requests (payloads are protobuf)
```

Error replies are standard micro errors, e.g. `Nats-Service-Error: deadline_exceeded: no answer within 3.0s` with `Nats-Service-Error-Code: 504`. So `nats micro stats` counts them.

## Observability

`scripts/run-all.sh` sets `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318`. The console exports its own spans through the Vite proxy (`/__otlp/spans`).

| What | Where |
|---|---|
| Traces: one per call, across every process, with log lines as span events | Jaeger http://localhost:16686 (service `wgic-console`, `visioner`, `sensor`, `llm_caller`) |
| Metric `rpc_server_call_duration_seconds` (histogram), labels `service_name`, `wgic_instance`, `rpc_service`, `rpc_method`, `rpc_response_status_code` | Prometheus http://localhost:9090 |
| Dashboard "RPC overview": calls/s, errors/s by code, p50/p95 latency by module and instance | Grafana http://localhost:3000 |
| Logs (OTLP, with trace ids) | `podman compose logs -f otel-collector`; also on each process's stdout |

`RUST_LOG` sets the log filter (default `info`).

## Check before you finish

```bash
scripts/verify.sh   # fmt, clippy, box rules, buf lint, unit + e2e tests (needs compose up), web verify
```
