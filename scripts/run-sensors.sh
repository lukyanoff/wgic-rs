#!/usr/bin/env bash
# Run one host process per sensor instance: 2 screens + 2 cameras. NATS only (no HTTP ports).
#
# Each instance gets its own NATS subject prefix (= instance id), so a call always reaches the device
# you mean (same-named services on the same subjects would be load-balanced).
#
#   screen-1  nats prefix screen-1   screen 0 (see ListDevices)
#   screen-2  nats prefix screen-2   screen 1
#   cam-1     nats prefix cam-1      camera 0
#   cam-2     nats prefix cam-2      camera 1
#
# Screens send JPEG (SCREEN_FORMAT, default jpeg): a PNG of a large screen can pass NATS' 8 MB limit and
# is too big for the LLMs the visioner sends it to.
# Needs NATS (`podman compose up -d`). Override NATS_URL to use another server.
# Ctrl+C stops all four.
set -euo pipefail
cd "$(dirname "$0")/.."

SCREEN_FORMAT="${SCREEN_FORMAT:-jpeg}"

export NATS_URL="${NATS_URL:-nats://127.0.0.1:4222}"
# Send traces, logs and metrics to the OTel collector from compose (Jaeger :16686 shows the traces).
export OTEL_EXPORTER_OTLP_ENDPOINT="${OTEL_EXPORTER_OTLP_ENDPOINT:-http://127.0.0.1:4318}"
source scripts/build-host.sh # sets $host

pids=()
stop_all() { kill "${pids[@]}" 2>/dev/null || true; }
trap stop_all EXIT INT TERM

# start <id> <env...>: the instance id is the NATS prefix, the NATS micro instance name
# (`nats micro ls` shows sensor-screen-1, ...) and the service name in Jaeger.
start() {
  local id=$1
  shift
  env SERVICES=sensor SENSOR_ID="$id" HTTP_DISABLED=1 NATS_PREFIX="$id" NATS_INSTANCE="$id" OTEL_SERVICE_NAME="$id" "$@" "$host" &
  pids+=($!)
}

start screen-1 SENSOR_KIND=screen SENSOR_DEVICE=0 SENSOR_FORMAT="$SCREEN_FORMAT"
start screen-2 SENSOR_KIND=screen SENSOR_DEVICE=1 SENSOR_FORMAT="$SCREEN_FORMAT"
start cam-1    SENSOR_KIND=camera SENSOR_DEVICE=0
start cam-2    SENSOR_KIND=camera SENSOR_DEVICE=1

echo "run-sensors: started screen-1, screen-2, cam-1, cam-2 on NATS (Ctrl+C stops all)"
wait
