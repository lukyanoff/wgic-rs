#!/usr/bin/env bash
# Run one host process per sensor instance: 2 screens + 2 cameras.
#
# Each instance gets its own HTTP port and its own NATS subject prefix, so a call always
# reaches the device you mean (same-named services on the same subjects would be load-balanced).
#
#   screen-1  http://127.0.0.1:8081  nats prefix screen-1   screen 0 (see ListDevices)
#   screen-2  http://127.0.0.1:8082  nats prefix screen-2   screen 1
#   cam-1     http://127.0.0.1:8083  nats prefix cam-1      camera 0
#   cam-2     http://127.0.0.1:8084  nats prefix cam-2      camera 1
#
# Needs NATS (`podman compose up -d`). Override NATS_URL to use another server.
# Ctrl+C stops all four.
set -euo pipefail
cd "$(dirname "$0")/.."

export NATS_URL="${NATS_URL:-nats://127.0.0.1:4222}"
cargo build --quiet -p host
host=target/debug/host

pids=()
stop_all() { kill "${pids[@]}" 2>/dev/null || true; }
trap stop_all EXIT INT TERM

# start <port> <service> <id> <env...>: the instance id doubles as the NATS prefix and the
# NATS micro instance name (`nats micro ls` then shows screen_sensor-screen-1, ...).
start() {
  local port=$1 service=$2 id=$3
  shift 3
  env SERVICES="$service" ADDR="127.0.0.1:$port" NATS_PREFIX="$id" NATS_INSTANCE="$id" "$@" "$host" &
  pids+=($!)
}

start 8081 screen_sensor screen-1 SCREEN_SENSOR_ID=screen-1 SCREEN_SENSOR_DEVICE=0
start 8082 screen_sensor screen-2 SCREEN_SENSOR_ID=screen-2 SCREEN_SENSOR_DEVICE=1
start 8083 camera_sensor cam-1    CAMERA_SENSOR_ID=cam-1    CAMERA_SENSOR_DEVICE=0
start 8084 camera_sensor cam-2    CAMERA_SENSOR_ID=cam-2    CAMERA_SENSOR_DEVICE=1

echo "run-sensors: started screen-1 :8081, screen-2 :8082, cam-1 :8083, cam-2 :8084 (Ctrl+C stops all)"
wait
