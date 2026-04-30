#!/usr/bin/env python3
"""Parse dashboard/settings/local.toml and emit shell-evaluable KEY=VALUE lines.

Used by `dashboard/scripts/infra_up.sh` (and any future infra script that
needs the same connection info) to keep the dashboard's runtime
configuration in a single source of truth.

Usage:
    parse_local_toml.py <path_to_local.toml>

Stdout (one KEY=value per line, suitable for `eval`):
    PG_HOST=localhost
    PG_PORT=5432
    PG_DB=reinhardt_cloud
    PG_USER=reinhardt
    PG_PASS=reinhardt
    RD_HOST=localhost
    RD_PORT=6379

Exit codes:
    0  success
    1  parse / validation error (details on stderr)
    2  invalid CLI usage
"""

from __future__ import annotations

import sys
import urllib.parse


def _load_toml(path: str) -> dict:
	try:
		import tomllib
	except ImportError:
		sys.stderr.write("Error: requires Python 3.11+ for tomllib\n")
		raise SystemExit(1)

	try:
		with open(path, "rb") as f:
			return tomllib.load(f)
	except FileNotFoundError:
		sys.stderr.write(f"Error: {path} not found\n")
		raise SystemExit(1)


def _resolve_password(raw: object) -> str:
	# Support both `password = "..."` and `password = { secret = "..." }`.
	if isinstance(raw, dict):
		raw = raw.get("secret", "")
	if not isinstance(raw, str) or not raw:
		sys.stderr.write(
			"Error: [core.databases.default].password must be a non-empty string\n"
		)
		raise SystemExit(1)
	return raw


def main(argv: list[str]) -> int:
	if len(argv) != 2:
		sys.stderr.write("Usage: parse_local_toml.py <local.toml>\n")
		return 2

	data = _load_toml(argv[1])

	try:
		db = data["core"]["databases"]["default"]
	except KeyError:
		sys.stderr.write("Error: [core.databases.default] missing from local.toml\n")
		return 1

	pw = _resolve_password(db.get("password", ""))
	redis_url = data.get("redis_url", "redis://localhost:6379/0")
	parsed = urllib.parse.urlparse(redis_url)

	print(f"PG_HOST={db.get('host', 'localhost')}")
	print(f"PG_PORT={db.get('port', 5432)}")
	print(f"PG_DB={db.get('name', 'reinhardt_cloud')}")
	print(f"PG_USER={db.get('user', 'reinhardt')}")
	print(f"PG_PASS={pw}")
	print(f"RD_HOST={parsed.hostname or 'localhost'}")
	print(f"RD_PORT={parsed.port or 6379}")
	return 0


if __name__ == "__main__":
	sys.exit(main(sys.argv))
