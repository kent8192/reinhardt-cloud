# Dashboard Dev Container

A pre-configured VS Code / Cursor dev container for working on the Reinhardt
Cloud dashboard. The container ships with the pinned Rust toolchain, every
cargo tool referenced by `dashboard/Makefile.toml`, and sidecar services
(PostgreSQL 17, Redis 7) wired up to the dashboard's settings.

## Prerequisites

- macOS, Linux, or Windows host with **Docker Desktop** running
- VS Code with the **Dev Containers** extension, or the [`devcontainer`
  CLI](https://github.com/devcontainers/cli)

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
| Sidecar services | `postgres:17-bookworm`, `redis:7-alpine` (compose, healthchecked) |
| Forwarded ports | `8000` (dashboard), `5432` (postgres), `6379` (redis) |

`/var/run/docker.sock` is bind-mounted so TestContainers can launch sibling
containers via the host's Docker daemon (docker-outside-of-docker).

## Settings layering

The container starts with `REINHARDT_ENV=devcontainer`, which causes the
Reinhardt configuration system to load `dashboard/settings/devcontainer.toml`
(copied from `devcontainer.example.toml` on first run). That file mirrors
`local.toml` but routes the DB and Redis hosts to the compose service names
(`postgres`, `redis`).

Both `local.toml` and `devcontainer.toml` are gitignored — the
`*.example.toml` templates are the canonical entries in version control.

## Caveats

- **Port collisions with `cargo make infra-up`**: if you also run the
  dashboard's `infra-up` task on the host, both setups try to bind 5432 and
  6379. Stop one before starting the other (`docker stop
  reinhardt-cloud-dashboard-postgres reinhardt-cloud-dashboard-redis` on the
  host, or shut the dev container down).
- **wasm-bindgen-cli version drift**: this container installs 0.2.118 to
  match `Cargo.lock`. Running `cargo make wasm-build-dev` may currently
  reinstall 0.2.114 because of an upstream pin in `dashboard/Makefile.toml`
  — that mismatch is tracked separately and does not break the build, only
  the cached binary.
- **First build is slow**: the cargo tools are compiled from source. Caching
  then makes container restarts effectively instant.

## Troubleshooting

- `Cannot connect to Docker daemon` from inside the container → ensure
  Docker Desktop is running on the host. The container reuses the host
  daemon by design.
- `IncompleteMessage` errors from TestContainers → confirm `DOCKER_HOST`
  inside the container points at `unix:///var/run/docker.sock` (it does by
  default; this only breaks if it gets manually overridden).
