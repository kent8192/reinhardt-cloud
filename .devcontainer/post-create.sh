#!/usr/bin/env bash
#
# Idempotent post-create hook for the dashboard dev container.
# Runs once after the container is built; safe to re-run.

set -euo pipefail

# 1. Ensure the wasm target is present. `rustup target add` is a no-op when
#    the target is already installed in the toolchain shipped by the image.
rustup target add wasm32-unknown-unknown

# 2. Sanity-check installed cargo tooling. Failure here means the Dockerfile
#    drifted from the dashboard's Makefile.toml expectations and we want
#    `Reopen in Container` to surface that immediately rather than producing
#    a half-broken environment that fails later inside `cargo make fmt-check`.
cargo make --version
cargo nextest --version
reinhardt-admin --version

# 3. Provision dashboard/settings/devcontainer.toml from the example template
#    on first run. The file is gitignored (matches `dashboard/settings/*.toml`),
#    so each developer ends up with a local copy they can edit.
if [ ! -f dashboard/settings/devcontainer.toml ]; then
	cp dashboard/settings/devcontainer.example.toml dashboard/settings/devcontainer.toml
	echo "Created dashboard/settings/devcontainer.toml from example."
fi

cat <<'EOF'

Dev container ready.

Next steps:
  cd dashboard
  cargo make migrate          # apply DB migrations
  cargo make runserver        # start the dashboard on http://localhost:8000

EOF
