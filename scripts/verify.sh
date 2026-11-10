#!/usr/bin/env bash
# Definition of done: format, lint, box rules, all tests (unit + integration + e2e), web.
# E2E needs the compose stack: `podman compose up -d`.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }

step "rust: format";  cargo fmt --all --check
step "rust: lint";    cargo clippy --workspace --all-targets --quiet -- -D warnings
step "boxes";         scripts/check-boxes.sh
step "proto: lint";   npx --no-install buf lint
step "rust: tests (unit + integration + e2e)"
cargo test --workspace --quiet
step "web: typecheck + lint + format"
(cd web && npm run --silent verify)

printf '\nverify: all green\n'
