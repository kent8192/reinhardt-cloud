#!/usr/bin/env bash
# Start disposable PostgreSQL + Redis containers via `docker run --rm`.
# Connection parameters (user / password / database / port / redis URL)
# are parsed from `dashboard/settings/local.toml`.
set -euo pipefail

PG_NAME="reinhardt-cloud-dashboard-postgres"
RD_NAME="reinhardt-cloud-dashboard-redis"

# Resolve dashboard/ from this script's location so the task works whether
# invoked through cargo-make or directly via `bash dashboard/scripts/infra_up.sh`.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DASHBOARD_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFIG="$DASHBOARD_DIR/settings/local.toml"

if [ ! -f "$CONFIG" ]; then
	echo "Error: $CONFIG not found." >&2
	echo "  Run: cp $DASHBOARD_DIR/settings/local.example.toml $CONFIG" >&2
	echo "       and fill in any required secrets before retrying." >&2
	exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
	echo "Error: python3 (>=3.11, for tomllib) is required to parse $CONFIG" >&2
	exit 1
fi

SETTINGS=$(CONFIG_PATH="$CONFIG" python3 - <<'PY'
import os, sys, urllib.parse
try:
	import tomllib
except ImportError:
	sys.stderr.write("Error: requires Python 3.11+ for tomllib\n")
	sys.exit(1)

with open(os.environ["CONFIG_PATH"], "rb") as f:
	data = tomllib.load(f)

try:
	db = data["core"]["databases"]["default"]
except KeyError:
	sys.stderr.write("Error: [core.databases.default] missing from local.toml\n")
	sys.exit(1)

pw = db.get("password", "")
# Support both `password = "..."` and `password = { secret = "..." }` forms.
if isinstance(pw, dict):
	pw = pw.get("secret", "")
if not isinstance(pw, str) or not pw:
	sys.stderr.write("Error: [core.databases.default].password must be a non-empty string\n")
	sys.exit(1)

redis_url = data.get("redis_url", "redis://localhost:6379/0")
parsed = urllib.parse.urlparse(redis_url)

print(f"PG_HOST={db.get('host', 'localhost')}")
print(f"PG_PORT={db.get('port', 5432)}")
print(f"PG_DB={db.get('name', 'reinhardt_cloud')}")
print(f"PG_USER={db.get('user', 'reinhardt')}")
print(f"PG_PASS={pw}")
print(f"RD_HOST={parsed.hostname or 'localhost'}")
print(f"RD_PORT={parsed.port or 6379}")
PY
)
eval "$SETTINGS"

# Drop any stale containers from a previous aborted run so --name is free.
docker rm -f "$PG_NAME" "$RD_NAME" >/dev/null 2>&1 || true

echo "Starting PostgreSQL ($PG_NAME) on ${PG_HOST}:${PG_PORT} as ${PG_USER}/${PG_DB}..."
docker run --rm -d \
	--name "$PG_NAME" \
	-p "${PG_PORT}:5432" \
	-e POSTGRES_USER="$PG_USER" \
	-e POSTGRES_PASSWORD="$PG_PASS" \
	-e POSTGRES_DB="$PG_DB" \
	postgres:17 >/dev/null

echo "Starting Redis ($RD_NAME) on ${RD_HOST}:${RD_PORT}..."
docker run --rm -d \
	--name "$RD_NAME" \
	-p "${RD_PORT}:6379" \
	redis:7-alpine >/dev/null

echo "Waiting for PostgreSQL..."
for _ in $(seq 1 30); do
	if docker exec "$PG_NAME" pg_isready -U "$PG_USER" -d "$PG_DB" >/dev/null 2>&1; then
		echo "  PostgreSQL ready"
		break
	fi
	sleep 1
done

echo "Waiting for Redis..."
for _ in $(seq 1 30); do
	if docker exec "$RD_NAME" redis-cli ping 2>/dev/null | grep -q PONG; then
		echo "  Redis ready"
		break
	fi
	sleep 1
done

echo "Infrastructure ready. Run 'cargo make infra-down' to stop."
