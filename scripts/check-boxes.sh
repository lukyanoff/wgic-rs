#!/usr/bin/env bash
set -euo pipefail

check_forbidden() {
  local crate="$1"
  local forbidden_regex="$2"
  local deps
  deps=$(cargo metadata --no-deps --format-version 1 | jq -r ".packages[] | select(.name == \"$crate\") | .dependencies[].name")
  if echo "$deps" | grep -Eq "$forbidden_regex"; then
    echo "Architecture violation in '$crate': forbidden dependency matched '$forbidden_regex'"
    exit 1
  fi
}

# Box 1 (contracts): pure abstractions & DTOs — zero internal dependencies
check_forbidden "contracts" "^(sdk|sensor|llm_caller|visioner|okf|host)$"

# Box 2 (sdk): NATS infrastructure — depends on contracts, never on services or host
check_forbidden "sdk" "^(sensor|llm_caller|visioner|okf|host)$"

# Box 3 (services): isolated domain boxes — depend only on contracts & sdk
for srv in sensor llm_caller visioner okf; do
  check_forbidden "$srv" "^(sensor|llm_caller|visioner|okf|host)$"
done

echo "All cognitive box boundaries verified!"
