#!/usr/bin/env bash
set -euo pipefail

HOST_BIN="./target/debug/host"
cargo build -p host

trap 'kill $(jobs -p) 2>/dev/null || true' EXIT

# Sensors
$HOST_BIN --module sensor --instance fake --config '{"kind":"fake"}' &
$HOST_BIN --module sensor --instance screen --config '{"kind":"screen","index":0}' &
# Cameras: "index" is N in /dev/videoN (`v4l2-ctl --list-devices`; each camera has a capture node and a metadata node).
$HOST_BIN --module sensor --instance cam --config '{"kind":"camera","index":0}' &    # integrated camera
$HOST_BIN --module sensor --instance usbcam --config '{"kind":"camera","index":2}' & # USB camera

# LLM Callers
$HOST_BIN --module llm_caller --instance agy --config '{"kind":"agy"}' &
$HOST_BIN --module llm_caller --instance claude --config '{"kind":"claude"}' &

# Visioner (Orchestrator) — 2 HA replicas sharing state `visioner.default`.
# Defaults: screen → agy. A request can override sensor (screen, cam, usbcam, fake) and llm_caller (agy, claude).
$HOST_BIN --module visioner --instance default \
  --config '{"sensor":"screen","llm_caller":"agy","user_prompt":"Describe what you see in this image."}' &
$HOST_BIN --module visioner --instance default \
  --config '{"sensor":"screen","llm_caller":"agy","user_prompt":"Describe what you see in this image."}' &

wait
