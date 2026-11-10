#!/usr/bin/env bash
set -euo pipefail

rm -f services/sensor/src/config_tests.rs \
      services/sensor/src/drivers_tests.rs \
      services/sensor/src/drivers/camera_tests.rs \
      services/llm_caller/src/drivers/agy_tests.rs \
      services/llm_caller/src/drivers/claude_tests.rs \
      services/llm_caller/src/drivers/cli_tests.rs \
      services/llm_caller/src/drivers/http_tests.rs \
      services/llm_caller/tests/drivers.rs \
      services/visioner/src/config_tests.rs

echo "Cleaned up leftover legacy files!"
