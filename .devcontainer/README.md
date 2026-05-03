# Dashboard Dev Container

A pre-configured VS Code / Cursor dev container for working on the Reinhardt
Cloud dashboard. The container ships with the pinned Rust toolchain, every
cargo tool referenced by `dashboard/Makefile.toml`, and sidecar services
(PostgreSQL 17, Redis 7) wired up to the dashboard's settings.

## Prerequisites

- macOS, Linux, or Windows host with a Docker-compatible runtime running
  (Docker Desktop, OrbStack, Colima, Rancher Desktop, or native Docker
  Engine on Linux all work)
- VS Code with the **Dev Containers** extension, or the [`devcontainer`
  CLI](https://github.com/devcontainers/cli)

### Host Docker socket location

The dev container bind-mounts the host's Docker socket so TestContainers
and `docker` CLI calls inside the container reach the host daemon. The
default mount source is `/var/run/docker.sock`, which works for Docker
Desktop and OrbStack out of the box. If your runtime exposes the socket
elsewhere, set `HOST_DOCKER_SOCKET` before bringing the container up:

```bash
# Colima
export HOST_DOCKER_SOCKET="$HOME/.colima/default/docker.sock"

# Rancher Desktop (default `dockerd-moby` on macOS)
export HOST_DOCKER_SOCKET="$HOME/.rd/docker.sock"
```

## Getting started

From the repository root:

```bash
# VS Code
code .
# Then: Command Palette -> "Dev Containers: Reopen in Container"

# Or with the CLI
devcontainer up --workspace-folder .
devcontainer exec --workspace-folder . zsh
```

The first build takes ~10–20 minutes because every cargo tool is compiled
from source rather than pulled as a prebuilt binary, which keeps the image
reproducible from a single pinned version list. Network access is still
required for the initial apt + crates.io fetches; subsequent rebuilds use
the Docker layer cache.

After the container is up:

```bash
cd dashboard
cargo make migrate          # apply DB migrations
cargo make runserver        # start the dashboard on http://localhost:8000
```

## What's inside

| Component | Detail |
| --- | --- |
| Base image | `mcr.microsoft.com/devcontainers/rust:1-bookworm` |
| Rust toolchain | 1.94.0 (matches `rust-toolchain.toml`) + `wasm32-unknown-unknown` |
| Cargo tools | `cargo-make`, `cargo-nextest`, `cargo-audit`, `bacon`, `wasm-bindgen-cli` (0.2.118), `reinhardt-admin-cli` (0.1.0-rc.15) |
| System tools | `protoc`, `binaryen` (`wasm-opt`), `lldb`, `postgresql-client`, `redis-tools`, `gh` |
| Sidecar services | `postgres:17-bookworm`, `redis:7-alpine` (compose, healthchecked, container-internal) |
| Forwarded ports | `8000` (dashboard) |

`/var/run/docker.sock` is bind-mounted so TestContainers can launch sibling
containers via the host's Docker daemon (docker-outside-of-docker).

## Settings layering

The container starts with `REINHARDT_ENV=local` and the env vars
`REINHARDT_DB_HOST=postgres` and `REINHARDT_REDIS_HOST=redis`. The
single `dashboard/settings/local.toml` file uses TOML interpolation
(kent8192/reinhardt-web#4092) — `host = "${REINHARDT_DB_HOST:-localhost}"` —
so it expands to the compose service name inside the container and falls
back to `localhost` for host-native development.

`local.toml` is gitignored; the `local.example.toml` template is the
canonical entry in version control. The `post-create` hook copies the
template on first container start.

## Caveats

- **PostgreSQL / Redis are container-internal**: the compose sidecars do
  not publish 5432 / 6379 on the host, so you cannot connect to them
  from the host with `psql -h localhost`. Use `docker compose exec
  postgres psql -U reinhardt reinhardt_cloud` (or the equivalent
  `redis-cli` invocation) instead. Running the dashboard's host-side
  `cargo make infra-up` in parallel is therefore safe; the two setups
  do not share ports.
- **wasm-bindgen-cli version drift**: this container installs 0.2.118 to
  match `Cargo.lock`. Running `cargo make wasm-build-dev` may currently
  reinstall 0.2.114 because of an upstream pin in `dashboard/Makefile.toml`
  — that mismatch is tracked separately and does not break the build, only
  the cached binary.
- **First build is slow**: the cargo tools are compiled from source. Caching
  then makes container restarts effectively instant.

## Troubleshooting

- `Cannot connect to Docker daemon` from inside the container → ensure
  the host Docker runtime is running and that `HOST_DOCKER_SOCKET` (if
  set) points at the runtime's actual socket. The container reuses the
  host daemon by design.
- TestContainers cannot find a Docker socket → check that
  `/var/run/docker.sock` exists *inside* the container (`docker run --rm
  -v /var/run/docker.sock:/var/run/docker.sock alpine ls -l
  /var/run/docker.sock` from the host is a fast sanity check). The
  bind-mount target is fixed; only the host-side source is configurable
  via `HOST_DOCKER_SOCKET`.
