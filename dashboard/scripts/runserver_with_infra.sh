#!/usr/bin/env bash
# Start local Reinhardt infrastructure and run the Dashboard server with the
# generated connection environment exported to child processes.
set -euo pipefail

infra_env="$(bash scripts/infra_up.sh --print-env)"

while IFS= read -r assignment; do
	if [[ "$assignment" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; then
		eval "export $assignment"
	fi
done <<< "$infra_env"

cargo run --locked --bin manage -- migrate
cargo run --locked --bin manage -- collectstatic --no-input
cargo run --locked --bin manage -- \
	runserver \
	--with-pages \
	--insecure \
	--static-dir \
	dist \
	--index \
	index.html
