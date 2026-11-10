# Connect RPC Rust example

Rust Connect/gRPC servers, plus a generic web playground that can call any
method on any server.
Based on https://connectrpc.com/docs/rust/getting-started/ and
https://connectrpc.com/docs/web/getting-started/

## Requirements

- Rust 1.88+
- Node.js: run `npm install` (repo root, for the Buf CLI) and `cd web && npm install`

## Generate code

Run this after you change or add any `.proto`:

```sh
npm run gen   # buf generate (Rust) + descriptors for the playground + cargo fmt
```

## Run the server and call it

```sh
cargo run -p host       # terminal 1: all services (SERVICES=greet runs only greet)
curl -H 'Content-Type: application/json' -d '{"name":"Jane"}' http://127.0.0.1:8080/greet.v1.GreetService/Greet
                        # terminal 2: {"greeting":"Hello, Jane!"}
```

The server listens on `127.0.0.1:8080` by default. To use another address, set `ADDR`:

```sh
ADDR=127.0.0.1:8081 cargo run -p host
```

More ways to call it (gRPC, NATS, the playground): see "Call it from the command line".

Servers allow CORS from `http://localhost:5173` by default. To change that, set
`CORS_ORIGINS` (comma-separated, or `*`):

```sh
CORS_ORIGINS=http://localhost:5173,https://playground.example.com cargo run -p host
```

## Web playground

Everything at once (compose, greet, sensors, LLM callers, playground; Ctrl+C stops it all):

```sh
scripts/run-all.sh
```

Add `--prod` for a release build with less memory per process (`scripts/run-all.sh --prod`). It sets
`HOST_PROFILE=release`, `MALLOC_ARENA_MAX=2`, `MALLOC_TRIM_THRESHOLD_=1048576` and `TOKIO_WORKER_THREADS=2`
(each can be overridden from the shell). Measured on a screen sensor after three 4K screenshots: 99 MB → 13 MB RSS,
21 → 4 threads. The release build takes about a minute the first time.

Or step by step:

```sh
npm run gen                  # once, and after proto changes
cd web && npm run dev        # http://localhost:5173
```

- **Methods:** every service in every proto in the repo, loaded from
  `web/public/descriptors.binpb`. You don't write code per service.
- **Servers:** the defaults come from `web/public/servers.json`. You can add,
  edit and remove servers in the UI (the "Edit" button), and changes are saved
  in your browser.
  - `proxy` mode goes through the Vite dev server, so no CORS is needed. It
    only works under `npm run dev`.
  - `direct` mode calls the server straight from the browser. It needs CORS on
    the server (see `CORS_ORIGINS`).
- **Protocols:** Connect (JSON or binary) and gRPC-web. Unary and
  server-streaming methods work. Client and bidi streaming can't run in a
  browser; use `buf curl` for those.
- **Editors:** the request editor starts from a template built from the message
  fields. Press Ctrl+Enter to send. Headers go one per line as `key: value`.
- **History:** the last 20 calls. Click one to restore it.

## Screen and camera sensors

`screen_sensor` and `camera_sensor` take a picture and return it with metadata (`sensor.v1.ImageMetadata`).
Both are built on `libs/sensor_base`, so they share config, encoding, logs/metrics and error codes.
Each has `Capture` and `ListDevices`.

**One instance per device.** Each sensor process captures one device, chosen by config. To run all four
(2 screens, 2 cameras), each in its own process with its own NATS prefix (NATS only, no HTTP ports; screens send
JPEG, `SCREEN_FORMAT=png scripts/run-sensors.sh` for PNG):

```sh
podman compose up -d
scripts/run-sensors.sh       # screen-1, screen-2, cam-1, cam-2 (Ctrl+C stops all)
podman compose run --rm nats-box nats req cam-2.camera_sensor.v1.CameraSensorService.Capture '{}'
```

In the playground, pick `screen-1 nats` … `cam-2 nats` (click "Reset to servers.json" once to get them).
Call `ListDevices` to see which index/name each device has, then edit the `*_DEVICE` lines in `scripts/run-sensors.sh`.

| Variable (screen / camera) | Values |
|---|---|
| `SCREEN_SENSOR_ID` / `CAMERA_SENSOR_ID` | instance id in metadata, logs, metrics (default `screen` / `camera`) |
| `SCREEN_SENSOR_DEVICE` / `CAMERA_SENSOR_DEVICE` | `default`, `0`, `1`, …, `name:<NAME>` |
| `SCREEN_SENSOR_FORMAT` / `CAMERA_SENSOR_FORMAT` | `png` (default), `jpeg`, `jpeg:<1..100>` |
| `SCREEN_SENSOR_DRIVER` | `auto` (= xcap), `xcap`, `linux-x11` |
| `CAMERA_SENSOR_DRIVER` | `auto`, `nokhwa-v4l`, `nokhwa-avfoundation`, `nokhwa-msmf` |
| `CAMERA_SENSOR_RESOLUTION` | `max` (default) or `<W>x<H>` |
| `CAMERA_SENSOR_WARMUP_FRAMES` | frames dropped before the picture (default 5) |

## LLM caller

`llm_caller` sends a chat (`[{role, content}]`, plus an optional JSON Schema) to an LLM.
Its one RPC, `Call`, is **server-streaming**: it sends `progress` messages (`started` /
`thinking` / `writing`, with the text received so far), then one `result` (the validated
JSON, the raw text and metadata such as tokens, cost and duration).

One service, four injected drivers. Each process is one instance with a fixed driver,
model, effort and temperature:

| Driver | How | Effort | Temperature |
|---|---|---|---|
| `claude-cli` | `claude -p --input-format stream-json --output-format stream-json`, no tools | `low`…`max` | no |
| `agy-cli` | `agy -p --output-format stream-json --sandbox --mode plan` | `low`…`max` | no |
| `openai` | `POST /v1/responses`, SSE (`OPENAI_API_KEY`) | `minimal`…`high` | yes |
| `openrouter` | `POST /api/v1/chat/completions`, SSE (`OPENROUTER_API_KEY`) | `minimal`…`high` | yes |

```sh
podman compose up -d
scripts/run-llm-callers.sh    # llm-claude, llm-agy, llm-openai, llm-openrouter on NATS (Ctrl+C stops all)
```

In the playground, pick `llm-claude nats` … `llm-openrouter nats` and call `Call`.
The progress messages appear one by one.

**Images.** A user message may carry `images: [{data, mime_type}]` (png, jpeg, webp, gif; ≤ 5 MB each):
claude gets them as base64 blocks on stdin, OpenAI/OpenRouter as data URLs, and agy (text-only stdin) as files
`image-1.png`… in the call's own temp folder, which its prompt names.

| Variable | Values |
|---|---|
| `LLM_CALLER_ID` | instance id in metadata, logs and metrics (default `llm`) |
| `LLM_CALLER_DRIVER` | `claude-cli`, `agy-cli`, `openai`, `openrouter`. Unset: every call answers `failed_precondition` |
| `LLM_CALLER_MODEL` | required, e.g. `claude-sonnet-5-5`, `gemini-3.8-flash-medium` (`agy models` lists them) |
| `LLM_CALLER_EFFORT` | see the table above (default `medium`) |
| `LLM_CALLER_TEMPERATURE` | `0..2`, HTTP drivers only. Set on a CLI driver, it stops the host at startup |
| `LLM_CALLER_TIMEOUT_SECS` | whole-call limit (default 300) |
| `LLM_CALLER_BASE_URL` / `LLM_CALLER_CLI_PATH` | replace the provider URL or the CLI program (tests, proxies) |

## Visioner (orchestrator)

`visioner` takes a screenshot from one screen sensor instance and asks K llm_caller instances, **in parallel**,
what they see. `Check` is server-streaming: progress from `screen` and from each model (its partial text, passed
on), then one result with every model's answer.

- Fail safe: a model that fails or times out shows up as `ok: false` with an `error_code`; the check fails only
  without a screenshot or when every model failed.
- Every call has a deadline (`connect-timeout-ms`). Servers enforce it on streams too, so a timed-out model call is
  stopped at the callee as well (llm_caller kills its CLI).
- It calls the other instances **by id over NATS**, through the client factories in `libs/clients`
  (`ScreenSensorClients::new(&deps).get("screen-1", timeout)`).

```sh
scripts/run-all.sh    # starts screen-1, the llm callers and vision-1 (= screen-1 + llm-claude,llm-agy)
```

In the playground, pick `vision-1 nats` and call `Check` from the **RPC** tab with **Timeout ms** ≈ 180000 (the
default 10 s ends a real check with `deadline_exceeded`). `Check` is streaming: the NATS tab and `nats req` send
unary `application/json` requests, which get `unimplemented` ("… is a streaming method"). In Jaeger (http://localhost:16686), service `vision-1`:
one trace holds the `Check` span with `screen-1`'s `Capture` and each model's `Call` under it.

| Variable | Values |
|---|---|
| `VISIONER_SCREEN` | screen sensor instance id, e.g. `screen-1`. Unset: every call answers `failed_precondition` |
| `VISIONER_LLMS` | llm_caller instance ids, comma-separated, e.g. `llm-claude,llm-agy` |
| `VISIONER_ID` | instance id in logs (default `vision`) |
| `VISIONER_SCREEN_TIMEOUT_SECS` / `VISIONER_LLM_TIMEOUT_SECS` | per-call deadlines (default 15 / 120) |

In `scripts/run-visioners.sh` the same settings are `VISION_SCREEN`, `VISION_LLMS`, … (e.g.
`VISION_LLMS=llm-claude,llm-openai scripts/run-all.sh`).

## NATS micro

Every service can also be hosted as a NATS micro service and called over NATS. No
per-service code is needed: `crates/platform/src/nats/` bridges NATS request/reply to the
same connect router.

```sh
podman compose up -d   # or docker compose; starts NATS from nats/nats.conf
                       # 4222 clients, 9222 WebSocket, 8222 monitoring

# server: HTTP on ADDR + NATS micro
NATS_URL=nats://127.0.0.1:4222 ADDR=127.0.0.1:8081 cargo run -p host
# NATS only (e.g. extra instances; they load-balance automatically)
HTTP_DISABLED=1 NATS_URL=nats://127.0.0.1:4222 cargo run -p host

# call over NATS (Connect JSON in, JSON out)
podman compose run --rm nats-box nats req rpc.greet.v1.GreetService.Greet '{"name":"Jane"}'
```

Server environment variables:

| Variable | Default | Meaning |
|---|---|---|
| `NATS_URL` | (unset, so NATS is off) | NATS server URL |
| `NATS_PREFIX` | `rpc` | Subject prefix |
| `NATS_SERVICE_VERSION` | crate version | Micro service version |
| `NATS_INSTANCE` | (unset) | Appended to each micro service name (`screen_sensor-screen-1`), so one module run for different devices shows as separate services. Unset: replicas share a name and load-balance |
| `HTTP_DISABLED` | (unset) | `1` turns the HTTP listener off |
| `SERVICES` | `all` | Comma-separated modules to run, e.g. `greet,billing` |
| `UPSTREAM_ADDR` | `ADDR` | Where services call other services over HTTP when `NATS_URL` is unset |

Each module is its own NATS micro service, named after the module (`greet`, or `greet-<NATS_INSTANCE>`). All
instances running that module share the name and load-balance.

**Wire format.** NATS carries Connect unary and server-streaming messages:

- **Subject:** `rpc.<package.Service>.<Method>`, e.g. `rpc.greet.v1.GreetService.Greet`
- **Endpoint name:** `GreetService-Greet`. Endpoint metadata `rpc` holds the RPC
  path.
- **Request:** the message in the payload. The `Content-Type` header is
  `application/json` (the default) or `application/proto`. Other headers are
  passed through as RPC metadata.
- **Error:** headers `Nats-Service-Error`, `Nats-Service-Error-Code` (HTTP
  status) and `Connect-Code`.
- **Server streaming:** the request has `Content-Type: application/connect+json` (or
  `+proto`) and a Connect-enveloped body, as an HTTP streaming call does. The server
  publishes every chunk of the Connect streaming body (envelopes, ending with the
  END_STREAM envelope) to the request's reply inbox. The first message also carries
  the response headers. It then sends an empty message with header `Nats-Stream-End: 1`.
  A failure before the stream starts comes back as an ordinary error reply. The Rust
  `NatsTransport` and the playground both read such replies; `nats req` shows only the
  first message.
- **Limits:** no client or bidi streaming (browsers can't send those anyway).

**Playground.** Pick the `nats ws` server (`ws://localhost:9222`, mode `nats`).
If you used the playground before, click "Reset to servers.json" to get it.
- The **RPC** tab calls methods through NATS WebSocket with Connect JSON or
  binary.
- The **NATS** tab is a browser version of the `nats` CLI. Each view shows the
  matching command, which you can copy.

| Playground (NATS tab) | CLI |
|---|---|
| Services | `nats micro ls`, `nats micro ls greet`, `nats micro info greet [id] [-e regex] [-j]` |
| Ping | `nats micro ping [greet]` (RTT per instance) |
| Stats (optional 2 s auto-refresh) | `nats micro stats greet [id] [-j]` |
| Request | `nats micro req greet GreetService-Greet '{"name":"Jane"}' [-H k:v] [--count N]` |
| Raw | `nats req <subject> <payload>` / `nats pub <subject> <payload>` |
| Subscribe | `nats sub 'rpc.>'` (live tail; `$SRV.>` shows discovery, `_INBOX.>` shows replies) |
| Server | `nats rtt`, plus the server INFO this connection received |

Request with count > 1 also shows a **load-distribution** chart: the
per-instance change in `num_requests`, taken from stats before and after the
run. On a Services endpoint, "Open in RPC" jumps to the typed RPC editor, and
"Request" pre-fills the Request view. A Subscribe keeps running while you use
the RPC tab, so you can watch your own calls.

**NATS CLI.** The CLI isn't installed on the host, so run it with
`podman compose run --rm --no-deps nats-box <command>`. `micro` is an alias of
`service`.

```sh
nats micro ls                      # services and instance ids
nats micro ls greet                # instances of one service
nats micro info greet              # endpoints, subjects, metadata, stats (-j for JSON)
nats micro ping greet              # every instance answers with its RTT
nats micro stats greet             # requests/errors/processing time per instance
nats micro req greet GreetService-Greet '{"name":"Jane"}'          # call by endpoint name
nats micro req greet GreetService-Greet '{"name":"x"}' --count 10  # load-balancing check
nats req rpc.greet.v1.GreetService.Greet '{"name":"Jane"}'         # raw subject
```

## Observability (OpenTelemetry)

Every request and reply is traced on every protocol: Connect JSON and binary,
gRPC and gRPC-web over HTTP, and Connect over NATS. Every log line is JSON
with the `trace_id` and `span_id` of the active span. Every call is measured
(`rpc.server.call.duration`), and services add their own metrics. The playground
starts each trace in the browser and shows the whole thing.

```sh
podman compose up -d   # NATS, otel-collector (4317 gRPC, 4318 HTTP), Jaeger (http://localhost:16686),
                       # Prometheus (http://localhost:9090), Grafana (http://localhost:3000)

OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318 OTEL_BSP_SCHEDULE_DELAY=200 \
  NATS_URL=nats://127.0.0.1:4222 ADDR=127.0.0.1:8081 cargo run -p host
# then call it from the playground (http://localhost:5173), which starts the trace in the browser
```

| Variable | Default | Meaning |
|---|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | (unset, so no export) | OTLP/HTTP collector. When it is unset, spans still get IDs, so logs stay correlated. |
| `OTEL_SERVICE_NAME` | `host` | `service.name` |
| `OTEL_BSP_SCHEDULE_DELAY` | `5000` | Batch export delay in ms. Use a low value so the playground sees server spans quickly. |
| `OTEL_METRIC_EXPORT_INTERVAL` | `10000` | Metrics export interval in ms |
| `RUST_LOG` | `info` | Log filter, e.g. `info,platform=debug` |

**Logs.** One JSON object per line on stdout. Each log is also an OTLP log
record (the collector prints them: `podman compose logs -f otel-collector`)
and an event on its span (Jaeger shows these as span logs):

```json
{"timestamp":"…","level":"INFO","target":"service","message":"greeting sent","category":"greet","name":"Jane","transport":"nats","span":"rpc.server","trace_id":"4491f568…","span_id":"d914f9f3…"}
```

Service fields (`kv("name", …)`) travel as one JSON string `log.fields`, because
`tracing` needs field names at compile time. The JSON formatter and the
collector (`transform/service_fields` in `otel/collector.yaml`) turn them back
into normal fields/attributes.

**Metrics.** Services → OTLP → collector (`:8889`) → Prometheus → Grafana. Grafana's
"RPC overview" dashboard (`grafana/dashboards/rpc-overview.json`) shows
requests/s, p50/p95 latency and errors per method and transport, plus the greet
service's own counters.

| Metric (Prometheus name) | Source | Labels |
|---|---|---|
| `rpc_server_call_duration_seconds` (histogram) | platform, every call | `rpc_system`, `rpc_service`, `rpc_method`, `rpc_transport`, `rpc_response_status_code` |
| `greet_greetings_total` | greet service | `transport` |
| `greet_rejected_total` | greet service | `reason` |

Every metric also has `service_name` (from `OTEL_SERVICE_NAME`).

**Spans.** W3C `traceparent`, `tracestate` and `baggage` are carried in HTTP
headers and in NATS message headers. Each hop rewrites them with its own span.

```
HTTP:  client span (playground / TracingTransport) → rpc.server
NATS:  client span → send <subject> (producer) → process <subject> (consumer) → rpc.server
```

`rpc.server` records `rpc.system`, `rpc.service`, `rpc.method`,
`rpc.wire_protocol` (connect | grpc | grpc_web), `rpc.transport` (http | nats),
`rpc.content_type` and, on failure, `rpc.connect_rpc.error_code`. It also logs
`rpc finished` / `rpc failed` with `duration_ms`.

**In code** (`crates/platform/src/telemetry/`):

- `platform::init_telemetry("name")` at the top of `main`. Keep the guard: it flushes on drop.
- Servers get `OtelInterceptor` (span + duration metric) automatically through
  `platform::run()`. Handlers run inside the `rpc.server` span, so logs are correlated.
- Services never call `tracing`/OTel: they get `deps.logger("svc")` and
  `deps.metrics("svc")` (`kernel::Logger` / `kernel::Metrics`). `deps_from_env()`
  backs them with `TracingLogs` and `OtelMetrics`; tests use `kernel::testing::{MemoryLogs, MemoryMetrics}`. The span's OTEL
  context is part of the request context:
  `kernel::RequestContextExt` gives `ctx.transport()`; infrastructure code can use
  `platform::telemetry::RequestContextOtel` for `ctx.otel()` / `ctx.trace_id()`.
- Clients: `platform::deps_from_env()` already traces calls. Manually, wrap any transport in `TracingTransport(…)`, e.g.
  `TracingTransport(HttpClient::plaintext())` or `TracingTransport(NatsTransport::new(nc, "rpc"))`.

**Playground.**

- Every call (RPC tab, and NATS Request/Raw) starts a trace in the browser
  (`service.name=connect-playground`) and exports it to the collector.
- A **Trace** panel appears under the response. It shows browser spans right
  away, then the whole trace from Jaeger: a span waterfall (click a span for
  its attributes and logs) and a **Logs** view with every log line from every
  service, ordered by time.
- NATS Request with `--count N` puts all N requests under one parent span.
  Subscribe rows that carry a `traceparent` open their trace.
- History rows link to the trace in Jaeger.
- The sidebar **Telemetry** panel shows the export status. You can edit the
  OTLP endpoint and the Jaeger URL there, or turn browser export off.
- Type a `traceparent` header to join an existing trace.
- Both directions go through the Vite dev server, so this needs `npm run dev`:
  - Browser spans are exported to `/__otlp/spans`, which is forwarded to
    `$OTEL_COLLECTOR_URL/v1/traces` (default `http://localhost:4318`). Because
    the export is same-origin, it needs no CORS, it works when the playground
    is opened from another host, and ad/privacy blockers that drop requests to
    `*/v1/traces` don't block it.
  - Jaeger is read through `/__jaeger`, forwarded to `$JAEGER_URL` (default
    `http://localhost:16686`).
  - You can also point the Telemetry panel straight at the collector (e.g.
    `http://localhost:4318/v1/traces`). The collector allows CORS from
    `http://localhost:5173` (`otel/collector.yaml`).

## Add a new service

Services follow the Cognitive Boxes layout (see `CLAUDE.md`). Copy `services/greet/`:

1. Add `proto/foo/v1/foo.proto`, run `npm run gen`, and add a `pub mod foo` block in
   `crates/contracts/src/lib.rs`.
2. Create `services/foo/` with the same files as `services/greet/`: `domain.rs` (rules as a trait + struct, e.g. `Greeter` / `HelloGreeter`),
   `domain_tests.rs` (unit tests), `handler.rs` (implements the generated `FooService`), `lib.rs`
   (`pub fn module(&Deps) -> Arc<dyn Module>`), `tests/rpc.rs` (integration), `CLAUDE.md`.
3. Register it: `foo = { path = "services/foo" }` in the root `[workspace.dependencies]`,
   `foo.workspace = true` in `apps/host/Cargo.toml`, and `foo::module(deps)` in
   `all_modules()` in `apps/host/src/main.rs`.
4. Add `tests/e2e/tests/foo.rs` (HTTP + NATS).
5. Run `scripts/verify.sh`. `scripts/check-boxes.sh` fails if any of the steps above is missing.

The host serves it over HTTP (`ADDR`) and NATS (`NATS_URL`). `SERVICES=foo` runs it alone. To
run more instances, start more processes. The playground lists it after a page reload.

## Call it from the command line

```sh
curl -H "Content-Type: application/json" -d '{"name":"Jane"}' \
  http://localhost:8080/greet.v1.GreetService/Greet

npx buf curl --schema . --protocol grpc --http2-prior-knowledge \
  --data '{"name":"Jane"}' http://localhost:8080/greet.v1.GreetService/Greet

# over NATS (needs NATS_URL on the host); subject = <prefix>.<package.Service>.<Method>
podman compose run --rm nats-box nats req rpc.greet.v1.GreetService.Greet '{"name":"Jane"}'
```

From Rust code, use the generated client with `platform::deps_from_env()` (HTTP or NATS, chosen by
env) — see `tests/e2e/src/lib.rs` for a working example.

## FYI: Buf Studio (no code)

Buf Studio is a hosted playground from Buf: https://buf.build/studio

1. Pick a schema. Either choose a module from the Buf Schema Registry, or push
   this repo's module with `npx buf push`. Studio needs the schema in the BSR.
2. Set the target URL, e.g. `http://127.0.0.1:8081`, and choose the Connect
   or gRPC-web protocol. Calls go straight from your browser, so they need the
   CORS above. Add `https://buf.build` to `CORS_ORIGINS`.
3. For plain gRPC, or to skip CORS, run a local agent:
   `npx buf beta studio-agent --port 8000`. Then set "Buf Studio Agent URL" in
   Studio to `http://localhost:8000`.

## Layout

Cognitive Boxes: every box has a `CLAUDE.md` describing what it owns and what it may import.
`scripts/check-boxes.sh` enforces the dependency rules.

- `proto/greet/v1/greet.proto`: the service definition
- `buf.yaml`, `buf.gen.yaml`: Buf module, lint settings and code generation plugins
- `crates/contracts`: generated code (`contracts::proto::...`)
- `crates/kernel`: what services see: `Module`, `Deps`, `Channel`, `Transport`
- `crates/platform`: infrastructure: `run()` (HTTP + NATS), CORS, `nats/` (micro adapter and
  `NatsTransport`), `telemetry/` (OTEL + JSON logs, `OtelInterceptor`, `TracingTransport`)
- `services/greet`: the greet service (domain, handler, tests)
- `apps/host`: the server binary (`SERVICES=...`)
- `tests/e2e`: end-to-end tests against the real `host` binary (needs `podman compose up -d`)
- `scripts/verify.sh`: definition of done (fmt, clippy, boxes, buf lint, all tests, web)
- `.claude/`: project hooks (format on edit, verify on stop) and the `simple-code` skill
- `compose.yaml`, `nats/nats.conf`: NATS with WebSocket and monitoring, plus nats-box (profile `tools`)
- `otel/collector.yaml`: OTEL Collector (OTLP in; traces to Jaeger, logs to its stdout, metrics to Prometheus)
- `prometheus/prometheus.yml`, `grafana/`: metrics storage, datasources and the "RPC overview" dashboard
- `web/`: the React playground
  - `src/schema.ts`: loads the descriptors
  - `src/call.ts`: dynamic calls
  - `src/nats.ts`: RPC calls over NATS WebSocket
  - `src/natsTools.ts`, `src/NatsConsole.tsx`: the NATS tab (the micro/req/pub/sub equivalents)
  - `src/template.ts`: request templates
  - `src/otel.ts`: the browser tracer, propagation helpers and the in-memory span store
  - `src/jaeger.ts`, `src/TracePanel.tsx`: read traces back from Jaeger and show them
  - `vite.config.ts`: the dev proxy, and `/__jaeger` for the Jaeger query API

## Gotchas

- The `connectrpc/rust` plugin needs the bare flag `file_per_package` in
  `buf.gen.yaml`. It fails on `file_per_package=true`.
- Running `buf generate` alone leaves the generated Rust unformatted. Use
  `npm run gen`.
- On this machine, port 8080 is used by a Podman container. Use
  `ADDR=127.0.0.1:8081`.


  ---
  How to run everything

Once, after cloning:
npm install                 # root: Buf CLI
(cd web && npm install)     # playground
npm run gen                 # generated code + web/public/descriptors.binpb

Terminal 1: infrastructure
podman compose up -d
This starts NATS (:4222), the OTel collector (:4318), Jaeger, Prometheus and Grafana.

Terminal 2: all services, over HTTP and NATS, with traces, logs and metrics exported
OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318 OTEL_BSP_SCHEDULE_DELAY=200 \
NATS_URL=nats://127.0.0.1:4222 ADDR=127.0.0.1:8081 \
cargo run -p host
- Only some services: add SERVICES=greet.
- Extra instances (they share the load over NATS): run the same command in another terminal with HTTP_DISABLED=1 instead of ADDR=....

Terminal 3: web playground
cd web && npm run dev

Call it
curl -H 'Content-Type: application/json' -d '{"name":"Jane"}' http://127.0.0.1:8081/greet.v1.GreetService/Greet
podman compose run --rm nats-box nats req rpc.greet.v1.GreetService.Greet '{"name":"Jane"}'
Or use the playground and pick the local 8081 or nats ws server.

Where to look

┌──────────────────────────────────────┬────────────────────────┐
│                 What                 │          URL           │
├──────────────────────────────────────┼────────────────────────┤
│ Playground                           │ http://localhost:5173  │
├──────────────────────────────────────┼────────────────────────┤
│ Traces (Jaeger)                      │ http://localhost:16686 │
├──────────────────────────────────────┼────────────────────────┤
│ Dashboards (Grafana, "RPC overview") │ http://localhost:3000  │
├──────────────────────────────────────┼────────────────────────┤
│ Metrics (Prometheus)                 │ http://localhost:9090  │
├──────────────────────────────────────┼────────────────────────┤
│ NATS monitoring                      │ http://localhost:8222  │
└──────────────────────────────────────┴────────────────────────┘

Check everything (the infrastructure must be up for the e2e tests):
scripts/verify.sh

Stop everything
- Ctrl+C the host and the playground.
- Then run podman compose down.
