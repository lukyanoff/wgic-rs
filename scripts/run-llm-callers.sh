#!/usr/bin/env bash
# Run one host process per LLM caller instance: the same llm_caller service, four injected drivers.
# NATS only (no HTTP ports).
#
#   llm-claude      nats prefix llm-claude      claude -p
#   llm-agy         nats prefix llm-agy         agy -p
#   llm-openai      nats prefix llm-openai      OpenAI (needs OPENAI_API_KEY)
#   llm-openrouter  nats prefix llm-openrouter  OpenRouter (needs OPENROUTER_API_KEY)
#
# Model / effort / temperature are fixed per instance: change them below, or override from the shell
# (LLM_CLAUDE_MODEL=claude-haiku-5-5 scripts/run-llm-callers.sh).
# Without an API key the HTTP instances still start and answer `failed_precondition`.
# Needs NATS (`podman compose up -d`). Ctrl+C stops all four.
set -euo pipefail
cd "$(dirname "$0")/.."

LLM_CLAUDE_MODEL="${LLM_CLAUDE_MODEL:-claude-sonnet-5-5}"
LLM_CLAUDE_EFFORT="${LLM_CLAUDE_EFFORT:-low}"
LLM_AGY_MODEL="${LLM_AGY_MODEL:-gemini-3.8-flash-medium}" # `agy models` lists the choices
LLM_AGY_EFFORT="${LLM_AGY_EFFORT:-medium}"
LLM_OPENAI_MODEL="${LLM_OPENAI_MODEL:-gpt-5-mini}"
LLM_OPENAI_EFFORT="${LLM_OPENAI_EFFORT:-low}"
LLM_OPENROUTER_MODEL="${LLM_OPENROUTER_MODEL:-openai/gpt-oss-120b}"
LLM_OPENROUTER_EFFORT="${LLM_OPENROUTER_EFFORT:-low}"
LLM_OPENROUTER_TEMPERATURE="${LLM_OPENROUTER_TEMPERATURE:-0.2}"

export NATS_URL="${NATS_URL:-nats://127.0.0.1:4222}"
# Send traces, logs and metrics to the OTel collector from compose (Jaeger :16686 shows the traces).
export OTEL_EXPORTER_OTLP_ENDPOINT="${OTEL_EXPORTER_OTLP_ENDPOINT:-http://127.0.0.1:4318}"
source scripts/build-host.sh # sets $host

pids=()
stop_all() { kill "${pids[@]}" 2>/dev/null || true; }
trap stop_all EXIT INT TERM

# start <id> <env...>: the instance id is also the NATS prefix, the NATS micro instance name
# (`nats micro ls` shows llm_caller-llm-claude, ...) and the service name in Jaeger.
start() {
  local id=$1
  shift
  env SERVICES=llm_caller HTTP_DISABLED=1 NATS_PREFIX="$id" NATS_INSTANCE="$id" OTEL_SERVICE_NAME="$id" LLM_CALLER_ID="$id" \
    "$@" "$host" &
  pids+=($!)
}

start llm-claude     LLM_CALLER_DRIVER=claude-cli LLM_CALLER_MODEL="$LLM_CLAUDE_MODEL"     LLM_CALLER_EFFORT="$LLM_CLAUDE_EFFORT"
start llm-agy        LLM_CALLER_DRIVER=agy-cli    LLM_CALLER_MODEL="$LLM_AGY_MODEL"        LLM_CALLER_EFFORT="$LLM_AGY_EFFORT"
start llm-openai     LLM_CALLER_DRIVER=openai     LLM_CALLER_MODEL="$LLM_OPENAI_MODEL"     LLM_CALLER_EFFORT="$LLM_OPENAI_EFFORT"
start llm-openrouter LLM_CALLER_DRIVER=openrouter LLM_CALLER_MODEL="$LLM_OPENROUTER_MODEL" LLM_CALLER_EFFORT="$LLM_OPENROUTER_EFFORT" \
  LLM_CALLER_TEMPERATURE="$LLM_OPENROUTER_TEMPERATURE"

echo "run-llm-callers: started llm-claude, llm-agy, llm-openai, llm-openrouter on NATS (Ctrl+C stops all)"
wait
