# SDK Crate Rules
- `server.rs`: `serve_endpoint` hosts a `RawRpcEndpoint` as a NATS micro service, enforces `Wgic-Timeout-Ms`
  (`MAX_CALL` cap), forwards progress (text batched every 250 ms), opens the `rpc.server` span and records
  `rpc.server.call.duration`.
- `client.rs`: `NatsRpcClient` (+ typed clients and `NatsSensorRegistry` / `NatsLlmRegistry`, built with a timeout).
  A call gets `min(timeout, time left of the call being served)`.
- `wire.rs`: reply-inbox frames (`0x01` step, `0x04` text, `0x02` ok) and `parse_service_error`.
- `telemetry.rs`: `init_telemetry(module, instance)` (keep the guard alive in `main`), `inject` / `continue_trace`.
- `state.rs`: `IStateStore<S>` with NATS KV CAS and `InMemoryStateStore`.
- Must NEVER import or reference any crate from `services/*` or `apps/host`.
