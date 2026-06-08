#!/usr/bin/env bash
# Recreate local infrastructure from a clean state.
set -euo pipefail

bash scripts/infra_down.sh
bash scripts/infra_up.sh
