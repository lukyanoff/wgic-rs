---
name: simple-code
description: Use for EVERY code change in this repo (Rust or TypeScript). Keeps code compact, flat and readable for C#/Python/JS/TS developers who are not Rust experts; minimizes cognitive depth. Read before writing or refactoring code, and when reviewing a diff.
---

# Simple code

Readers are C#, Python and TypeScript developers. They should understand a function in one pass,
without knowing advanced Rust. Optimize for **reading**, not for cleverness or fewest characters.

## Shape

- **Functions ≤ 40 lines, files ≤ 300 lines, nesting ≤ 3 levels.** Clippy (`too_many_lines` = 50,
  `excessive_nesting`) and oxlint enforce the hard limits; aim lower.
- **Early return** instead of nested `if`/`match`: `let Some(x) = y else { return ...; };`
- **One idea per function.** Name it after what it returns or does: `parse_connect_error`, not `process`.
- **Pure core, thin edges.** Rules go in `domain.rs`.
  Handlers only translate proto ⇄ domain.
- **≤ 4 parameters.** More? Pass a named struct.
- **Line width 120** (`rustfmt.toml`, `web/.oxfmtrc.json`); the formatter decides wrapping — don't fight it.

## Rust: prefer the boring version

| Avoid | Prefer |
|---|---|
| iterator chains longer than ~3 steps, `fold` with complex state | a `for` loop with a named `mut` variable |
| custom macros, `macro_rules!` | functions |
| explicit lifetimes (`'a`) in your own types | owned data (`String`, `.clone()` of small things) |
| generic functions with trait bounds | concrete types (`Channel`, `Arc<dyn Module>`) |
| tuples in public APIs `(String, u16, bool)` | a named struct |
| `unwrap()` in non-test code | `?`, or `expect("why this can't fail")` |
| `impl Trait` in return position across modules | a named type or type alias |
| clever `Option`/`Result` combinator chains | `match` or `let ... else` |

## Errors: two kinds, two tools

| Where | Use | Why |
|---|---|---|
| service domain (`services/*`) | one small `enum` per service (`GreetError`) with `Display` | the handler must map each case to a Connect code (`invalid_argument`, `not_found`…) with a `match` the compiler checks |
| infrastructure + apps (`platform`, `apps/*`, `tests/e2e`) | `anyhow::Result<T>`, `bail!("...")`, `.context("doing X")` | errors are only logged or returned from `main`; nobody matches on them |

- Never `anyhow` in a service crate, and never `Box<dyn Error>` anywhere.
- Add context where the failure happens: `.with_context(|| format!("connect to NATS at {url}"))`.
- In `handler.rs`, one function `to_connect_error(e: XError) -> ConnectError` does the mapping.

## TypeScript

- Plain functions and small components; a component over ~60 lines gets split.
- No `any`; prefer `type` aliases and discriminated unions over class hierarchies.
- Same rules: early returns, ≤ 4 params (use an options object), no clever one-liners.

## Tests

- Tests never share a file with code. `foo.rs` ends with:
  ```rust
  #[cfg(test)]
  #[path = "foo_tests.rs"]
  mod tests;
  ```
  and `foo_tests.rs` (same folder) starts with `use super::*;`, so it can test private functions.
- One behaviour per test, named as a sentence: `empty_name_is_invalid_argument`.
- Arrange / act / assert, with small helper functions (`client()`, `request("Jane")`) instead of setup frameworks.

## Domain: interface + class (C# style)

The domain is a **trait (interface) + struct (class)**, injected into the handler — not free functions.

```rust
// domain.rs
pub trait Greeter: Send + Sync {                       // ≈ interface IGreeter
    fn greet(&self, name: &str) -> Result<String, GreetError>;
}

pub struct HelloGreeter { max_name_chars: usize }     // ≈ class Greeter : IGreeter
impl HelloGreeter { pub fn new() -> Self { ... } }    // ≈ constructor
impl Greeter for HelloGreeter { fn greet(...) { ... } }

// handler.rs — depends on the interface
pub struct GreetHandler { greeter: Arc<dyn Greeter>, log: Logger, metrics: Metrics }

// lib.rs::module() — the composition root: the only place that picks implementations
let handler = GreetHandler::new(deps, Arc::new(HelloGreeter::new()));
```

- Naming: interface = the role (`Greeter`, `OrderRepository`), class = what it is (`HelloGreeter`, `PostgresOrderRepository`). No `I` prefix.
- Inject as `Arc<dyn Trait>` (≈ a reference to an interface). Construct only in `module()`.
- Configuration goes in the struct's fields via `new()` / `with_...()` constructors, not globals.
- Keep traits small (1–3 methods, one role). The domain struct never logs or measures; the handler does.
- Unit-test the class directly (`HelloGreeter::new().greet("Jane")`); pass a fake `impl Greeter` where the handler logic needs isolating.

## Logging & metrics (injected, never static)

Services never call `tracing::info!` or OTel directly. Inject in the constructor, like C# DI:

```rust
pub struct OrderHandler { log: Logger, metrics: Metrics }

impl OrderHandler {
    pub fn new(deps: &Deps) -> Self {
        Self { log: deps.logger("orders"), metrics: deps.metrics("orders") }
    }
}

self.log.info("order placed", [kv("order_id", id), kv("items", count)]);
let log = self.log.with([kv("order_id", id)]);          // scope: added to every entry
self.metrics.count("placed", 1, [kv("channel", "web")]); // → orders.placed (orders_placed_total)
self.metrics.record("pricing_seconds", 0.012, []);       // histogram; `_seconds` gets latency buckets
self.metrics.set("open_carts", 42.0, []);                 // gauge
```

| C# | here |
|---|---|
| `ILogger<T>` | `deps.logger("svc")` → `Logger` |
| `logger.LogInformation("Placed {OrderId}", id)` | `log.info("order placed", [kv("order_id", id)])` |
| `logger.BeginScope(...)` | `log.with([kv(...)])` |
| `IMetrics` / `Meter.CreateCounter` | `deps.metrics("svc")` → `count` / `record` / `set` by name |

- Messages are short constant phrases (`"order placed"`), data goes in fields — never `format!` data into the message.
- Metric names are short (`placed`); the meter adds `svc.`. Never use a name with two kinds.
- Metric tags must have few values (transport, reason, status). **Never** ids, names, emails.
- Don't add per-RPC duration/count/error metrics: platform records them for every call.
- Log/measure in the handler (or an application `service.rs`); `domain.rs` stays pure.
- Tests: `Deps::in_process(&[]).with_logs(logs.clone()).with_metrics(metrics.clone())` with
  `kernel::testing::{MemoryLogs, MemoryMetrics}`, then `logs.find("order placed")`, `metrics.sum("orders.placed")`.

## Comments

- Doc comment on every `pub` item: what it does, with a tiny example when the shape isn't obvious.
- Inline comments explain **why**, never what. Delete comments that restate the code.

## Rust ↔ C# / TS cheat sheet (use these mental models in docs)

| Rust | C# | TS |
|---|---|---|
| `trait` | `interface` | `interface` |
| `Arc<dyn Greeter>` field | `IGreeter` constructor parameter | `greeter: Greeter` field |
| `struct` + `impl` | `class` | `class` / object type |
| `enum` with data | sealed record hierarchy | discriminated union |
| `Result<T, E>` + `?` | exception / `TryX` pattern | `throw` / result type |
| `Option<T>` | `T?` | `T \| undefined` |
| `Arc<T>` | shared reference | object reference |
| `async fn` / `.await` | `async Task` / `await` | `async` / `await` |
| `foo_tests.rs` (`#[path]` test module) | `FooTests` class | `foo.test.ts` |

## Before you finish

1. Could a C# developer explain each new function after one read? If not, simplify.
2. Did you stay inside one box (see root `CLAUDE.md`)?
3. Tests at every layer the change touches (unit / integration / e2e).
4. `scripts/verify.sh` is green (the Stop hook will also check).
