# Contracts Crate Rules (Box 1: Pure Abstractions)
- Pure C#-style `Contracts.Abstractions` crate.
- Contains ONLY Protobuf DTO structs (`prost`), service traits (`ISensorService`, `ILlmCallerService`, `IVisionerService`), `RpcError`, `ProgressSender`, `ClientRegistry<T>`, and `RawRpcEndpoint`.
- ZERO infrastructure dependencies: must NEVER depend on `sdk`, `async-nats`, `services/*`, or `apps/host`.
