# SDK Crate Rules
- Provides transport (`NatsRpcClient`, `serve_endpoint`), `StreamFrame`, `IStateStore<S>`, and `ClientRegistry<T>`.
- Must NEVER import or reference any crate from `services/*` or `apps/host`.
