#!/usr/bin/env bash
# Enforce the Cognitive Boxes rules (see CLAUDE.md):
#   1. dependency arrows between workspace crates point only where allowed
#   2. tests are in their own files (foo.rs + foo_tests.rs), never inline
#   3. every service has unit tests, integration tests, e2e tests and a CLAUDE.md
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
error() { echo "check-boxes: $*" >&2; fail=1; }

# Which workspace crates each box may depend on (normal + dev dependencies).
allowed_for() {
  case "$1" in
    crates/contracts) echo "" ;;
    crates/kernel)    echo "" ;;
    crates/platform)  echo "kernel" ;;
    services/*)       echo "contracts kernel" ;;
    tests/e2e)        echo "contracts kernel platform" ;;
    apps/*)           echo "*" ;;
    *)                echo "?" ;;
  esac
}

root="$PWD"
# One line per workspace crate: "<dir> <path-dep> <path-dep> ..."
cargo metadata --format-version 1 --no-deps | jq -r --arg root "$root/" '
  .packages[]
  | [(.manifest_path | ltrimstr($root) | rtrimstr("/Cargo.toml"))]
    + [.dependencies[] | select(.path != null) | .name]
  | join(" ")' |
while read -r dir deps; do
  allowed=$(allowed_for "$dir")
  [ "$allowed" = "*" ] && continue
  if [ "$allowed" = "?" ]; then
    echo "check-boxes: $dir is not in a known box (crates/, services/, apps/, tests/e2e)" >&2
    echo FAIL; continue
  fi
  for dep in $deps; do
    if [[ " $allowed " != *" $dep "* ]]; then
      echo "check-boxes: $dir must not depend on '$dep' (allowed: ${allowed:-nothing})" >&2
      echo FAIL
    fi
  done
done | grep -q FAIL && fail=1

# Every service: unit + integration + e2e tests + its own CLAUDE.md.
for svc in services/*/; do
  name=$(basename "$svc")
  ls "$svc"src/*_tests.rs >/dev/null 2>&1 || error "$name: no unit tests (services/$name/src/<file>_tests.rs)"
  ls "$svc"tests/*.rs >/dev/null 2>&1   || error "$name: no integration tests (services/$name/tests/*.rs)"
  [ -f "tests/e2e/tests/$name.rs" ]     || error "$name: no e2e tests (tests/e2e/tests/$name.rs)"
  [ -f "$svc/CLAUDE.md" ]               || error "$name: no services/$name/CLAUDE.md"
  grep -qs "$name::module" apps/host/src/main.rs || error "$name: not registered in apps/host/src/main.rs"
  grep -qs "^anyhow" "$svc/Cargo.toml" && error "$name: services use their own error enum, not anyhow (see simple-code skill)"
done

# Tests live in their own files: `foo.rs` + `foo_tests.rs`, never an inline `mod tests { ... }`.
inline=$(grep -rlE '^\s*mod tests \{' crates services apps tests --include='*.rs' | grep -v '/src/gen/' || true)
for f in $inline; do
  error "$f: move the inline 'mod tests { ... }' into a sibling *_tests.rs file"
done

[ "$fail" = 0 ] && echo "check-boxes: ok"
exit "$fail"
