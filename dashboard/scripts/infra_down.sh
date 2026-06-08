#!/usr/bin/env bash
# Stop local infrastructure started by `manage infra` plus the temporary Redis
# compatibility container.
set -euo pipefail

cargo run --locked --bin manage -- infra down
docker stop reinhardt-cloud-dashboard-redis >/dev/null 2>&1 || true
