#!/usr/bin/env bash
# The NATS CLI (nats-box container) against the compose NATS, from any folder.
#   scripts/nats.sh micro ls
#   scripts/nats.sh micro stats visioner
#   scripts/nats.sh sub 'rpc.>'
set -euo pipefail
cd "$(dirname "$0")/.."
# -T (no TTY) when not in a terminal, so it also works in pipes and scripts.
tty_flag=()
[ -t 0 ] && [ -t 1 ] || tty_flag=(-T)
exec podman compose run --rm "${tty_flag[@]}" nats-box nats "$@" 2> >(grep -v -e 'external compose provider' -e ' Container ' >&2)
