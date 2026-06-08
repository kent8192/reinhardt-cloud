#!/usr/bin/env bash
# Start local infrastructure through `manage infra`.
set -euo pipefail

PRINT_ENV=0
if [[ "${1:-}" == "--print-env" ]]; then
	PRINT_ENV=1
	shift
fi

if [[ "$#" -ne 0 ]]; then
	echo "Usage: scripts/infra_up.sh [--print-env]" >&2
	exit 2
fi

REDIS_CONTAINER="reinhardt-cloud-dashboard-redis"
REDIS_PORT="${RC_REDIS_PORT:-6379}"
REDIS_HOST="${RC_REDIS_HOST:-localhost}"
REDIS_DATABASE="${RC_REDIS_DATABASE:-1}"
REDIS_URL_VALUE="redis://${REDIS_HOST}:${REDIS_PORT}/${REDIS_DATABASE}"

shell_quote() {
	printf "%q" "$1"
}

extract_env_value() {
	local key="$1"
	local line
	while IFS= read -r line; do
		if [[ "$line" == "${key}="* ]]; then
			eval "printf '%s' \"\${line#${key}=}\""
			return 0
		fi
	done <<< "$infra_env"
	return 1
}

infra_env="$(cargo run --locked --bin manage -- infra up --print-env)"

if [[ "$PRINT_ENV" -eq 1 ]]; then
	printf "%s\n" "$infra_env"
else
	cargo run --locked --bin manage -- infra status
fi

if grep -Eq "^(REDIS_URL|REINHARDT_REDIS_URL)=" <<< "$infra_env"; then
	if [[ "$PRINT_ENV" -eq 1 && -z "${REINHARDT_CLOUD_REDIS_URL:-}" ]]; then
		redis_env_value="$(extract_env_value REDIS_URL || extract_env_value REINHARDT_REDIS_URL)"
		printf "REINHARDT_CLOUD_REDIS_URL=%s\n" "$(shell_quote "$redis_env_value")"
	fi
	exit 0
fi

# Workaround for kent8192/reinhardt-web#5213 (tracked in
# kent8192/reinhardt-cloud#678). Remove this workaround when `manage infra`
# derives Redis from resolved application settings.
#
# Ideal implementation (without workaround):
#   cargo run --locked --bin manage -- infra up --print-env
#   # The command provisions Redis and emits the Redis environment overrides.
docker rm -f "$REDIS_CONTAINER" >/dev/null 2>&1 || true
docker run --rm -d \
	--name "$REDIS_CONTAINER" \
	-p "${REDIS_PORT}:6379" \
	redis:7-alpine >/dev/null

for _ in $(seq 1 30); do
	if docker exec "$REDIS_CONTAINER" redis-cli ping 2>/dev/null | grep -q PONG; then
		if [[ "$PRINT_ENV" -eq 1 ]]; then
			printf "REDIS_URL=%s\n" "$(shell_quote "$REDIS_URL_VALUE")"
			printf "REINHARDT_REDIS_URL=%s\n" "$(shell_quote "$REDIS_URL_VALUE")"
			printf "REINHARDT_CLOUD_REDIS_URL=%s\n" "$(shell_quote "$REDIS_URL_VALUE")"
		else
			echo "redis ${REDIS_HOST}:${REDIS_PORT} -> 6379"
		fi
		exit 0
	fi
	sleep 1
done

echo "Error: Redis did not become ready in ${REDIS_CONTAINER}" >&2
exit 1
