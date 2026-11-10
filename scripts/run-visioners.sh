#!/usr/bin/env bash
# Run the visioner (orchestrator) instance. NATS only (no HTTP port).
#
#   vision-1  nats prefix vision-1  screen from VISION_SCREEN, models VISION_LLMS (asked in parallel)
#
# It calls the other instances by id over NATS, so start them first: scripts/run-sensors.sh and
# scripts/run-llm-callers.sh (or everything at once: scripts/run-all.sh).
# Needs NATS (`podman compose up -d`). Ctrl+C stops it.
set -euo pipefail
cd "$(dirname "$0")/.."

VISION_SCREEN="${VISION_SCREEN:-screen-1}"
VISION_LLMS="${VISION_LLMS:-llm-claude,llm-agy}"
VISION_SCREEN_TIMEOUT_SECS="${VISION_SCREEN_TIMEOUT_SECS:-15}"
VISION_LLM_TIMEOUT_SECS="${VISION_LLM_TIMEOUT_SECS:-120}"

export NATS_URL="${NATS_URL:-nats://127.0.0.1:4222}"
# Send traces, logs and metrics to the OTel collector from compose (Jaeger :16686 shows the traces).
export OTEL_EXPORTER_OTLP_ENDPOINT="${OTEL_EXPORTER_OTLP_ENDPOINT:-http://127.0.0.1:4318}"
source scripts/build-host.sh # sets $host

id=vision-1
env SERVICES=visioner HTTP_DISABLED=1 NATS_PREFIX="$id" NATS_INSTANCE="$id" OTEL_SERVICE_NAME="$id" VISIONER_ID="$id" \
  VISIONER_SCREEN="$VISION_SCREEN" VISIONER_LLMS="$VISION_LLMS" \
  VISIONER_SCREEN_TIMEOUT_SECS="$VISION_SCREEN_TIMEOUT_SECS" VISIONER_LLM_TIMEOUT_SECS="$VISION_LLM_TIMEOUT_SECS" \
  "$host" &
trap 'kill $! 2>/dev/null || true' EXIT INT TERM

echo "run-visioners: started $id on NATS: screen $VISION_SCREEN, models $VISION_LLMS (Ctrl+C stops it)"
wait
