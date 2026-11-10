# Box 5 — e2e

Black-box tests: build and start the real `host` binary, call it through generated clients over
**HTTP and NATS**. Needs the compose stack (`podman compose up -d`); tests fail with a clear
message when NATS is down.

- Harness: `src/lib.rs` → `Host::start("<services>")`, `host.http_deps()`, `host.nats_deps().await`.
  Each host gets a free port and a unique NATS subject prefix, so tests run in parallel and never
  hit dev servers.
- One file per service: `tests/<service>.rs` (required by `scripts/check-boxes.sh`).
- Cover: happy path on both transports + at least one error code across NATS.
- Test only through public RPCs — never import a service crate here.
- Run: `cargo test -p e2e` or `cargo test -p e2e --test <service>`.
