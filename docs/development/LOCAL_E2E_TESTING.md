# Local End-to-End Testing Guide

This guide walks a developer from a fresh clone to a locally running Reinhardt
Cloud stack — Dashboard, Operator, Agent, and a Kubernetes cluster — and
exercises the deploy paths that the codebase supports today.

The Zero Config Deploy flow spans four processes:

```
reinhardt-cloud (CLI) --> Dashboard (HTTP + gRPC) --> Agent (in-cluster gRPC client) --> kubectl apply CRD --> Operator reconcile
```

Until the follow-up work listed in [Known limitations](#known-limitations) lands,
the Dashboard-mediated leg is wired to an in-process mock, so commands from the
Dashboard do not reach the Agent binary. The `--dry-run` and `--direct` paths
are fully exercised end-to-end.

## Audience

Contributors running the stack locally for the first time. This guide assumes
familiarity with Rust, `cargo`, Docker, and `kubectl`.

## Prerequisites

| Tool | Notes |
|---|---|
| Docker Desktop | **Required.** Podman is not supported (see `.testcontainers.properties` and `CLAUDE.md`). Confirm with `docker ps`. |
| `kubectl` | Any recent version. |
| Rust toolchain | Pinned by `rust-toolchain.toml` in the repo root. |
| `cargo-make` | `cargo install cargo-make` |
| `cargo-nextest` | `cargo install cargo-nextest --locked` |
| Local Kubernetes | Either [`kind`](https://kind.sigs.k8s.io/) **or** [OrbStack](https://orbstack.dev/) Kubernetes (choose one). |

Helpful checks before starting:

```bash
docker ps
kubectl version --client
cargo --version
```

## Automated Dashboard self-deploy harness

For the Dashboard dogfood path, prefer the first-class harness before walking
the manual checklist:

```bash
cargo make dashboard-self-deploy-e2e
```

The task builds or selects the Dashboard image, applies the `ReinhardtApp`
CRD, creates a temporary namespace and runtime Secret, starts a local Operator
when no in-cluster Operator is already installed, generates the Dashboard
`ReinhardtApp` with `reinhardt-cloud deploy --dir dashboard --dry-run`, requires
Dashboard `manage introspect` to succeed, applies the same contract through
`--direct`, waits for the Operator-owned Deployment, Service, cache/database
resources, Pods, and live `ReinhardtApp`, seeds an active Dashboard user with
its Personal Organization, verifies login through the deployed frontend server
function, checks authenticated Dashboard pages, then removes the temporary
namespace.

Useful overrides:

| Variable | Default | Purpose |
|---|---|---|
| `DASHBOARD_SELF_DEPLOY_NAMESPACE` | `reinhardt-dashboard-e2e-<timestamp>` | Reuse or name the test namespace. |
| `DASHBOARD_SELF_DEPLOY_IMAGE` | `reinhardt-cloud-dashboard:e2e` | Dashboard image used in the generated CRD. |
| `DASHBOARD_SELF_DEPLOY_BUILD_IMAGE` | `1` | Set to `0` to use an already-built image. |
| `DASHBOARD_SELF_DEPLOY_OPERATOR_MODE` | `auto` | `auto`, `existing`, `local`, or `skip`. |
| `DASHBOARD_SELF_DEPLOY_OPERATOR_METRICS_ADDR` | `127.0.0.1:19090` | Metrics/health bind address for the local Operator process. |
| `DASHBOARD_SELF_DEPLOY_OPERATOR_BIN` | `target/debug/reinhardt-cloud-operator` | Override the local Operator binary path. |
| `DASHBOARD_SELF_DEPLOY_CLI_BIN` | `target/debug/reinhardt-cloud` | Override the local CLI binary path. |
| `DASHBOARD_SELF_DEPLOY_MANAGE_BIN` | `target/debug/manage` | Override the Dashboard `manage` binary used for strict introspection. |
| `DASHBOARD_SELF_DEPLOY_REINHARDT_ENV` | `ci` | `REINHARDT_ENV` used by local Dashboard `manage introspect`. |
| `DASHBOARD_SELF_DEPLOY_KEEP_RESOURCES` | `0` | Set to `1` to keep the namespace after the run. |
| `DASHBOARD_SELF_DEPLOY_INTROSPECT_TIMEOUT_SECONDS` | `30` | Timeout for each CLI `manage introspect` attempt. The harness fails instead of using zero-config fallback. |
| `DASHBOARD_SELF_DEPLOY_ARTIFACT_DIR` | `target/dashboard-self-deploy-e2e/<namespace>` | Failure diagnostics and generated YAML. |
| `DASHBOARD_SELF_DEPLOY_KUBECTL_CONTEXT` | current context | Kubernetes context for `kubectl`. |
| `DASHBOARD_SELF_DEPLOY_KIND_CLUSTER` | inferred from `kind-*` context | Explicit `kind load docker-image` target. |
| `DASHBOARD_SELF_DEPLOY_E2E_USERNAME` | `e2e-user` | Username seeded inside the deployed Dashboard Pod for authenticated flow checks. |
| `DASHBOARD_SELF_DEPLOY_E2E_PASSWORD` | `e2e-password-123456` | Password assigned to the seeded Dashboard user. |
| `DASHBOARD_SELF_DEPLOY_E2E_EMAIL` | `e2e@example.test` | Email assigned to the seeded Dashboard user. |
| `DASHBOARD_SELF_DEPLOY_PORT_FORWARD_PORT` | `18080` | Local port used for Dashboard health, login, and authenticated page checks. |
| `DASHBOARD_SELF_DEPLOY_ORIGIN` | `http://127.0.0.1:8000` | Origin/Referer used for server function POSTs. The default matches the CI `OriginGuardMiddleware` allow-list. |

When running the Dashboard development server on a port other than 8000,
set `PORT` to the served port or add the exact origin under
`[cors].allow_origins`. In debug profiles the router augments configured
origins with `http://localhost:$PORT` and `http://127.0.0.1:$PORT` so
same-origin server-function POSTs do not fail OriginGuard validation after
switching to ports such as 8001.

On failure the harness writes events, live `ReinhardtApp` YAML, owned resource
YAML, Pod logs, Operator logs, and the generated CRD YAML under the artifact
directory before cleanup. Login responses, cookies, and authenticated page
responses are also preserved there. This harness validates the generated-config
→ `--direct` → Operator reconciliation → authenticated Dashboard contract. The
Dashboard → Agent relay is still limited as described in
[Known limitations](#known-limitations).

## 1. Bring up a local Kubernetes cluster

Pick one of the two options.

### Option A — kind

```bash
kind create cluster --name reinhardt-local
kubectl cluster-info --context kind-reinhardt-local
```

### Option B — OrbStack

Enable Kubernetes in OrbStack (Settings → Kubernetes → Enable), then:

```bash
kubectl config use-context orbstack
kubectl cluster-info
```

## 2. Start dependency services

Postgres and Redis are launched as ephemeral containers (`docker run --rm`)
via the cargo-make task defined in the workspace-root `Makefile.toml`.
Data is wiped when the containers stop, which keeps each local session
isolated. (`cargo make runserver` from the workspace root will also start
these containers automatically as a dependency, so this step is only
needed when you want the infra without launching the dev server.)

```bash
cargo make infra-up
docker ps --filter name=reinhardt-cloud-dashboard-
```

`infra-up` reads connection parameters from the settings TOML matching
the active `REINHARDT_ENV` profile (defaults to `local`, so `local.toml`).
This is the same lookup `runserver` performs, so both sides stay in
sync. With a typical `local.toml` the defaults are
`postgres://reinhardt:reinhardt@localhost:5432/reinhardt_cloud` and
`redis://localhost:6379`. Stop the containers with `cargo make infra-down`,
or recreate from a clean state with `cargo make infra-reset`.

> **Note:** if you have `REINHARDT_ENV` set in your shell (e.g. left over
> from a CI shell where you exported `REINHARDT_ENV=ci`), both `infra-up`
> and `runserver` will resolve `<env>.toml` instead of `local.toml`. Run
> `unset REINHARDT_ENV` before running `infra-up` to use the default
> `local` profile.

## 3. Install the CRD

Resolved by kent8192/reinhardt-cloud#315 — the CRD YAML now ships with the
Helm chart:

```bash
kubectl apply -f charts/reinhardt-cloud-operator/crds/reinhardtapp-crd.yaml
kubectl get crd reinhardtapps.paas.reinhardt-cloud.dev
```

## 4. Run the Dashboard

The Dashboard is the HTTP API (`:8000`) and gRPC control plane (`:50051`).
It uses the reinhardt framework's `manage` binary (migrations and runserver
come from the framework, not from a crate-local subcommand).

### 4a. Apply migrations

```bash
export REINHARDT_CLOUD_REDIS_URL="redis://localhost:6379"
cargo run -p reinhardt-cloud-dashboard --bin manage -- migrate
```

### 4b. Start the server

In a dedicated terminal:

```bash
export REINHARDT_CLOUD_REDIS_URL="redis://localhost:6379"
cargo run -p reinhardt-cloud-dashboard --bin manage -- runserver
```

Required / useful environment variables:

| Variable | Purpose |
|---|---|
| `REINHARDT_CLOUD_REDIS_URL` | Required. Validated at startup by `RedisValidationHook` in `dashboard/src/config/hooks.rs`. |
| `REINHARDT_CLOUD_CONFIG_DIR` | Optional. Overrides the directory scanned for settings TOML. |

The gRPC server is spawned alongside HTTP by `GrpcRunserverHook`
(`dashboard/src/config/hooks.rs`) and binds to `127.0.0.1:50051` using
`GrpcServerConfig::default()` (`crates/reinhardt-cloud-grpc/src/config.rs`).

## 5. Run the Operator

In another terminal:

```bash
cargo run -p reinhardt-cloud-operator
```

The Operator uses the host's `KUBECONFIG` to reach the cluster started in
step 1. `rustls` `CryptoProvider` is installed explicitly at startup
(kent8192/reinhardt-cloud#314) — no TLS panic on Kubernetes 1.31+.

## 6. Run the Agent

In another terminal:

```bash
cargo run -p reinhardt-cloud-agent -- \
  --cluster-name local-cluster \
  --control-plane-url http://127.0.0.1:50051
```

| Flag / env var | Default | Purpose |
|---|---|---|
| `--control-plane-url` / `CONTROL_PLANE_URL` | `http://127.0.0.1:50051` | Dashboard gRPC endpoint. |
| `--cluster-name` / `CLUSTER_NAME` | (required) | Arbitrary label used in streamed events. |
| `--node-name` / `NODE_NAME` | `unknown` | Reported as the node identifier. |
| `--heartbeat-interval` | `30` | Seconds between heartbeats. |
| `--auth-token` / `AUTH_TOKEN` | unset | Bearer JWT for `AgentServiceClient`. See `crates/reinhardt-cloud-grpc/src/interceptor.rs` for the claims shape. |

Field sources: `crates/reinhardt-cloud-agent/src/main.rs`.

The Agent reads the host's `KUBECONFIG` (via `kube::Client::try_default()`) and
issues a bidirectional `AgentStream` RPC to the Dashboard. You should see
`Starting Reinhardt Cloud Agent` and then heartbeat sends in its logs.

## 7. Register the cluster in the Dashboard

A Dashboard account is needed to create a cluster. The CLI ships `login` and
`credentials` commands that persist a JWT to `~/.config/reinhardt-cloud/credentials.json`
(`crates/reinhardt-cloud-cli/src/config.rs`):

```bash
cargo run -p reinhardt-cloud-cli -- login
cargo run -p reinhardt-cloud-cli -- credentials show
```

Then create a cluster record. Until a CLI subcommand exists for this, call the
Dashboard directly:

```bash
TOKEN=$(jq -r .token ~/.config/reinhardt-cloud/credentials.json)
curl -X POST http://localhost:8000/clusters/ \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"local-cluster","api_url":"https://kubernetes.default.svc","is_active":true}'
```

Note: the `Cluster` model (`dashboard/src/apps/clusters/models/cluster.rs`) does
**not** currently issue an Agent `AUTH_TOKEN`. Any bearer token used for the
Agent must be minted out-of-band against the JWT secret configured for the
Dashboard's gRPC interceptor — see Known limitations below.

## 8. Exercise the deploy flows

All three modes are wired in `crates/reinhardt-cloud-cli/src/commands/deploy.rs`.
Use a minimal `reinhardt-cloud.toml` in a scratch project directory, or override
with CLI flags.

### 8a. `--dry-run` (YAML only)

```bash
cargo run -p reinhardt-cloud-cli -- deploy \
  --name demo --image nginx:1.27 --replicas 2 --dry-run
```

Expected: a `ReinhardtApp` CRD YAML on stdout. No cluster or Dashboard calls.

### 8b. `--direct` (skip Dashboard, apply CRD directly)

```bash
cargo run -p reinhardt-cloud-cli -- deploy \
  --name demo --image nginx:1.27 --replicas 2 --direct
kubectl get reinhardtapp -A -w
```

Expected: the CRD is applied; the Operator reconciles it and creates the
backing Deployment / Service.

### 8c. Default path (via Dashboard)

```bash
cargo run -p reinhardt-cloud-cli -- deploy \
  --name demo --image nginx:1.27 --replicas 2
```

Expected: the Dashboard returns a 2xx for `POST /deployments`. The CLI
includes the generated `ReinhardtApp` YAML in the request body as
`reinhardt_app_yaml`, but the Dashboard does not yet persist or relay that
manifest to the Agent. **The Agent binary will not receive the deploy command
yet** — see the first item in Known limitations.

## Known limitations

These are the gaps between the documented layout and a fully wired
Dashboard → Agent → Operator path. Each has a tracking Issue:

1. **Dashboard gRPC uses `MockClusterAgentService`**
   (`dashboard/src/config/grpc.rs` line `49`). Streams from a live Agent
   terminate at the mock; deploy commands issued through the Dashboard do
   not reach the Agent's `handle_command` loop. Tracked in
   kent8192/reinhardt-cloud#360.
2. **No Agent Dockerfile.** The Agent runs on the host against the
   cluster's API server. Full in-cluster testing requires an image.
   Tracked in kent8192/reinhardt-cloud#358.
3. **No Agent Helm chart.** `charts/` contains only the Operator chart;
   there is no production-shaped `reinhardt-cloud-agent` chart.
   Tracked in kent8192/reinhardt-cloud#359.
4. **No `AUTH_TOKEN` issuance on cluster registration.**
   `dashboard/src/apps/clusters/views/create_cluster.rs` does not emit an
   Agent bearer token. `AUTH_TOKEN` must currently be minted manually using
   the JWT secret that `crates/reinhardt-cloud-grpc/src/interceptor.rs`
   validates against. Tracked in kent8192/reinhardt-cloud#361.

Until each item is resolved, treat the "Dashboard-mediated deploy" scenario
(section 8c) as a partial smoke test: HTTP path and payload acceptance only.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `rustls CryptoProvider` panic at Operator/Agent startup | Already patched; rebuild and rerun. See kent8192/reinhardt-cloud#314. |
| `gRPC connection refused` from Agent | Dashboard's `runserver` is not up, or `REINHARDT_CLOUD_REDIS_URL` is unset (server exits at startup). Check the Dashboard terminal. |
| `Cannot connect to Docker daemon` during integration tests | `DOCKER_HOST` is pointing at Podman. `unset DOCKER_HOST` or install Docker Desktop. See `CLAUDE.md` → "Troubleshooting Container Errors". |
| `kubectl apply` fails with `no matches for kind "ReinhardtApp"` | CRD is not installed. Re-run section 3. |
| `cargo run -p reinhardt-cloud-dashboard` rebuilds on every invocation | Use a dedicated terminal per long-running component so incremental compilation stays warm. |

## References

- Architecture overview: `README.md`
- Kubernetes operator patterns: `instructions/KUBERNETES_PATTERNS.md`
- Proto contract for the Agent stream: `crates/reinhardt-cloud-proto/proto/cluster_agent.proto`
- Agent binary source: `crates/reinhardt-cloud-agent/src/main.rs`
- Dashboard gRPC wiring: `dashboard/src/config/grpc.rs`, `dashboard/src/config/hooks.rs`
- CLI deploy logic: `crates/reinhardt-cloud-cli/src/commands/deploy.rs`
- Dashboard cluster endpoints: `dashboard/src/apps/clusters/`
