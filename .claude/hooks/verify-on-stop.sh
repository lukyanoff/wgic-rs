#!/usr/bin/env bash
# Stop / SubagentStop hook: the agent can't finish while scripts/verify.sh fails.
# - Skips when the working tree is identical to the last green run (Q&A turns are free).
# - After 5 failed attempts in a row it lets the agent stop and tells the human.
set -uo pipefail
input=$(cat)
cd "${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}" || exit 0

state_dir="$(git rev-parse --git-dir)/claude-verify"
mkdir -p "$state_dir"
session=$(jq -r '.session_id // "default"' <<<"$input")
attempts_file="$state_dir/attempts-$session"

# Fingerprint of all uncommitted work (tracked diffs + untracked files).
fingerprint=$(
  {
    git rev-parse HEAD
    git diff HEAD
    git ls-files --others --exclude-standard -z | xargs -0 -r sha1sum
  } | sha1sum | cut -d' ' -f1
)
if [ "$fingerprint" = "$(cat "$state_dir/last-green" 2>/dev/null)" ]; then
  exit 0
fi

log="$state_dir/last-run.log"
if scripts/verify.sh >"$log" 2>&1; then
  echo "$fingerprint" >"$state_dir/last-green"
  rm -f "$attempts_file"
  exit 0
fi

attempts=$(( $(cat "$attempts_file" 2>/dev/null || echo 0) + 1 ))
echo "$attempts" >"$attempts_file"
if [ "$attempts" -ge 5 ]; then
  rm -f "$attempts_file"
  echo '{"systemMessage": "scripts/verify.sh still fails after 5 attempts; the agent was allowed to stop. Full log: .git/claude-verify/last-run.log"}'
  exit 0
fi
{
  echo "scripts/verify.sh failed (attempt $attempts/5). You are not done: fix this, then finish."
  echo "Full log: .git/claude-verify/last-run.log. Last lines:"
  tail -n 60 "$log"
} >&2
exit 2
