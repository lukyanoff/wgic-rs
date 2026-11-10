#!/usr/bin/env bash
# Build and run everything for local work, in one terminal. Services talk over NATS only (no HTTP ports):
#
#   infrastructure  podman compose: NATS, OTel collector, Jaeger :16686, Prometheus :9090, Grafana :3000
#   greet           nats prefix rpc
#   sensors         scripts/run-sensors.sh      (screen-1, screen-2, cam-1, cam-2)
#   llm callers     scripts/run-llm-callers.sh  (llm-claude, llm-agy, llm-openai, llm-openrouter)
#   visioner        scripts/run-visioners.sh    (vision-1: screen-1 + llm-claude, llm-agy)
#   playground      http://localhost:5173
#
# Usage: scripts/run-all.sh [--prod]
#   (default)  debug build: fast to rebuild, full backtraces
#   --prod     release build + lower memory per process (see "Profile" below)
#
# Env passes through to the scripts (OPENAI_API_KEY, LLM_CLAUDE_MODEL, ...). Ctrl+C stops all of it;
# the compose containers keep running (`podman compose down` stops them).
set -euo pipefail
cd "$(dirname "$0")/.."

# ---------------------------------------------------------------------------------------------
# Profile
# ---------------------------------------------------------------------------------------------

case "${1:-}" in
  --prod)
    # Optimized binary (scripts/build-host.sh): less code mapped into every process.
    export HOST_PROFILE=release
    # glibc keeps freed memory per thread ("arenas"); a 4K screenshot then stays reserved after capture.
    # Fewer arenas + trimming freed memory back to the OS keep the sensors small between captures.
    export MALLOC_ARENA_MAX="${MALLOC_ARENA_MAX:-2}"
    export MALLOC_TRIM_THRESHOLD_="${MALLOC_TRIM_THRESHOLD_:-1048576}"
    # tokio starts one worker thread per CPU core by default; these services need only a few.
    export TOKIO_WORKER_THREADS="${TOKIO_WORKER_THREADS:-2}"
    ;;
  "") ;;
  *)
    echo "usage: $0 [--prod]" >&2
    exit 2
    ;;
esac

export NATS_URL="${NATS_URL:-nats://127.0.0.1:4222}"
# Send traces, logs and metrics to the OTel collector from compose (Jaeger :16686 shows the traces).
export OTEL_EXPORTER_OTLP_ENDPOINT="${OTEL_EXPORTER_OTLP_ENDPOINT:-http://127.0.0.1:4318}"

# Ctrl+C or any exit: stop every process this script started (they share its process group).
trap 'trap - EXIT INT TERM; kill 0 2>/dev/null' EXIT INT TERM

# ---------------------------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------------------------

podman compose up -d
source scripts/build-host.sh # sets $host
echo "run-all: profile ${HOST_PROFILE:-debug} ($host)"
if [ ! -d web/node_modules ]; then
  npm --prefix web install
fi

listening() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }

# The hosts connect to NATS at startup, so wait until it accepts connections.
for _ in $(seq 1 30); do
  if listening 4222; then
    break
  fi
  sleep 1
done

# ---------------------------------------------------------------------------------------------
# Run
# ---------------------------------------------------------------------------------------------

SERVICES=greet HTTP_DISABLED=1 NATS_PREFIX=rpc NATS_INSTANCE=greet OTEL_SERVICE_NAME=greet "$host" &
scripts/run-sensors.sh &
scripts/run-llm-callers.sh &
scripts/run-visioners.sh &
# A playground left running is used as it is, not restarted.
if listening 5173; then
  echo "run-all: :5173 is in use, playground not started (using the one already running)"
else
  npm --prefix web run dev -- --strictPort &
fi

echo "run-all: greet, sensors, llm callers, vision-1 on NATS; playground http://localhost:5173 (Ctrl+C stops all)"
wait
