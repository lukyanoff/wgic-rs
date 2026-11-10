#!/usr/bin/env bash
# PostToolUse hook (Edit|Write): format the file that was just written. Never blocks.
file=$(jq -r '.tool_input.file_path // empty')
root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}"
case "$file" in
  "$root"/crates/contracts/src/gen/*) ;;  # generated; `npm run gen` formats it
  *.rs) rustfmt --edition 2024 "$file" 2>/dev/null ;;
  "$root"/web/src/*) (cd "$root/web" && npx --no-install oxfmt "$file" >/dev/null 2>&1) ;;
esac
exit 0
