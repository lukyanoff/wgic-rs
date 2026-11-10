# WGIC-RS Architecture & AI Agent Rules

This repository implements a distributed, NATS-native actor/service mesh in Rust using **strict SOLID principles**, **Constructor Dependency Injection**, and **Isolated Cognitive Boxes**.

## 1. The 4 Cognitive Boxes (Strict Dependency Graph)

```text
apps/host (Box 4: Composition Root — CLI, config parsing, DI wiring)
  ├──> services/* (Box 3: Business Logic — sensor, llm_caller, visioner)
  │      ├──> crates/sdk
  │      └──> crates/contracts
  └──> crates/sdk (Box 2: Infrastructure — NATS RPC, StreamFrame, CAS StateStore, Telemetry)
         └──> crates/contracts (Box 1: Protobuf DTOs + Service/Client Traits)
```

### Boundary Rules (Enforced by `scripts/check-boxes.sh`)
1. **`crates/contracts`**: Contains ONLY `prost` Protobuf structs, service traits (`ISensorService`, `ILlmCallerService`, `IVisionerService`), NATS client adapters, and `RawRpcEndpoint` dispatchers. Must NEVER depend on `services/*` or `apps/host`.
2. **`crates/sdk`**: Contains NATS transport (`NatsRpcClient`, `serve_endpoint`), `StreamFrame` (`0x01` Progress, `0x02` Ok, `0x03` Err), `IStateStore<S>` (Compare-And-Swap over NATS KV + `InMemoryStateStore`), `ClientRegistry<T>`, and `tracing` initialization. Must NEVER know what specific services exist.
3. **`services/*`**: Pure, isolated domain crates.
   - Must depend ONLY on `crates/contracts`, `crates/sdk`, and domain-specific driver libraries.
   - Must NEVER depend on `async-nats` directly or on other `services/*` crates.
4. **`apps/host`**: The sole **Composition Root** (analogous to `Program.cs` in .NET). Parses CLI flags (`--module`, `--instance`, `--config`), builds concrete drivers/registries/KV stores, injects them into service constructors, and mounts endpoints on NATS.

---

## 2. Mandatory Coding Rules for AI Agents (C# / SOLID Style in Rust)

1. **True Constructor DI Only (No Service Locator):**
   - Never pass a `Deps` or `Context` bag into handler methods.
   - Every service handler (`SensorHandler`, `LlmCallerHandler`, `VisionerHandler`) must receive all external dependencies (`Arc<dyn Driver>`, `Arc<dyn IStateStore<S>>`, `Arc<dyn ClientRegistry<dyn IService>>`) via `pub fn new(...)`.
2. **Unified RPC Signature with `ProgressSender`:**
   - Do not split unary vs. streaming traits. Every RPC trait method takes `(req: Req, progress: ProgressSender) -> Result<Res, RpcError>`.
   - Call `progress.report("stage description")` to emit real-time progress frames (`0x01`) to the caller.
3. **Compare-And-Swap (CAS) Shared Instance State:**
   - Every service instance owns a typed state struct persisted in NATS KV via `Arc<dyn IStateStore<State>>`.
   - Always mutate state using `self.state.update_cas(Box::new(|s| { ... })).await` so multiple High-Availability (HA) replicas sharing an `--instance` ID never overwrite each other's updates.
4. **Standard `tracing` Only (Zero Custom Logger/Meter Boilerplate):**
   - Never create custom `Logger` or `Meter` traits.
   - Annotate RPC methods with `#[tracing::instrument(skip(self, req, progress))]` and log events using standard `tracing::info!`, `tracing::warn!`, and `tracing::error!`.
5. **No Explicit Lifetimes in Business Logic:**
   - Keep structs and traits `'static` using owned types (`String`, `Vec<u8>`, `Arc<dyn Trait>`).
6. **RAII Resource Cleanup:**
   - Never manage temporary files with manual `fs::remove_file` or atomic counters. Always use `tempfile::Builder` so files are cleaned up automatically on `Drop`.
7. **Unit Tests alongside Implementation:**
   - Place unit tests in `src/*_tests.rs` wired via `#[cfg(test)] mod ..._tests;`.
   - Test handlers using `InMemoryStateStore` and `MapRegistry` mocks injected via `new()`—unit tests must run in <10ms with zero network or NATS dependencies.

---

## 3. NATS Subject & HA Conventions

- **RPC Subject:** `rpc.<module>.<instance>.<proto_package.ServiceName>.<MethodName>`
  - Example: `rpc.visioner.default.visioner.v1.VisionerService.Capture`
- **Queue Group (HA Load Balancing):** `q.<module>.<instance>`
  - Multiple OS processes started with the same `--module` and `--instance` automatically share load via NATS Queue Groups and share state via NATS KV key `<module>.<instance>` in bucket `wgic_instance_state`.
- **Wire Framing (`sdk::wire::StreamFrame`):**
  - `0x01` + UTF-8 string -> Progress frame (0 or more per call)
  - `0x02` + Protobuf bytes -> Terminal OK response
  - `0x03` + `"<code>: <message>"` UTF-8 string -> Terminal Error response

---

## 4. Verification Workflow

Before finishing any task, run:
```bash
./scripts/check-boxes.sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --exclude e2e
```
