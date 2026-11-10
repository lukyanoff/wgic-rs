#!/usr/bin/env bash
set -euo pipefail

HOST_BIN="./target/debug/host"
cargo build -p host

trap 'kill $(jobs -p) 2>/dev/null || true' EXIT

# Sensors
$HOST_BIN --module sensor --instance fake --config '{"kind":"fake"}' &
$HOST_BIN --module sensor --instance screen --config '{"kind":"screen","index":0}' &

# LLM Callers
$HOST_BIN --module llm_caller --instance agy --config '{"kind":"agy"}' &
$HOST_BIN --module llm_caller --instance claude --config '{"kind":"claude"}' &

# Visioner (Orchestrator) — 2 HA replicas sharing state `visioner.default`
$HOST_BIN --module visioner --instance default \
  --config '{"sensor":"fake","llm_caller":"agy","user_prompt":"Describe this image"}' &
$HOST_BIN --module visioner --instance default \
  --config '{"sensor":"fake","llm_caller":"agy","user_prompt":"Describe this image"}' &

wait
