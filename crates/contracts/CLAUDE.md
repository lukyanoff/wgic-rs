# Box 0 — contracts

Generated Rust code for every `.proto` in `/proto`. **No hand-written logic.**

- Source of truth: `proto/<package>/v1/*.proto`. Edit those, then run `npm run gen` (repo root).
- `src/gen/` is generated — never edit it. `src/lib.rs` only has one `pub mod` block per proto package.
- Proto style: `buf lint` (STANDARD) must pass. Breaking changes: `npx buf breaking --against '.git#branch=main'`.
- Use from other boxes: `contracts::proto::<package>::v1::{Message, XService, XServiceClient}`.
- Must not depend on any other workspace crate.
