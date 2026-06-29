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

## Fast Dashboard frontend E2E

The default test command includes the Dashboard's headless Chrome WASM browser
tests:

```bash
cargo make test
```

Those tests launch the real Dashboard WASM client in the browser and use
Reinhardt's `MockServiceWorker` to intercept server-function fetches. For a
Dashboard-only run, use:

```bash
cd dashboard
cargo make wasm-spa-test
```

The `test` tasks declare both the native suite and the WASM browser suite as
`cargo-make` dependencies, so a successful `cargo make test` run includes both.

## Automated Dashboard self-deploy harness

For the Dashboard dogfood path, prefer the first-class harness before walking
the manual checklist:

```bash
cargo make dashboard-self-deploy-e2e
```

The task builds or selects the Dashboard image, applies the `Project`
CRD, creates a temporary namespace and runtime Secret, starts a local Operator
when no in-cluster Operator is already installed, generates the Dashboard
`Project` with `reinhardt-cloud deploy --dir dashboard --dry-run`, requires
Dashboard `manage introspect` to succeed, applies the same contract through
`--direct`, waits for the Operator-owned Deployment, Service, cache/database
resources, revision migration Job, Pods, and live `Project`, seeds an active Dashboard user with
its Personal Organization, verifies login through the deployed frontend server
function, checks authenticated Dashboard route shells, then removes the temporary
namespace.

By default, the harness first uses the configured Kubernetes context when it is
reachable. If the current context is stopped or missing and `kind` is installed,
it creates or reuses a local `reinhardt-dashboard-e2e` kind cluster and loads the
Dashboard image there. Set `DASHBOARD_SELF_DEPLOY_CLUSTER_MODE=existing` to keep
the older fail-fast behavior.

Useful overrides:

| Variable | Default | Purpose |
|---|---|---|
| `DASHBOARD_SELF_DEPLOY_NAMESPACE` | `reinhardt-dashboard-e2e-<timestamp>` | Reuse or name the test namespace. |
| `DASHBOARD_SELF_DEPLOY_IMAGE` | `reinhardt-cloud-dashboard:e2e` | Dashboard image used in the generated CRD. |
| `DASHBOARD_SELF_DEPLOY_BUILD_IMAGE` | `1` | Set to `0` to use an already-built image. |
| `DASHBOARD_SELF_DEPLOY_DOCKERFILE` | `dashboard/Dockerfile` | Dockerfile template used for the Dashboard image build. |
| `DASHBOARD_SELF_DEPLOY_RUST_VERSION` | nearest `rust-toolchain.toml` channel | Rust image version used when preparing the build Dockerfile. |
| `DASHBOARD_SELF_DEPLOY_OPERATOR_MODE` | `auto` | `auto`, `existing`, `local`, or `skip`. |
| `DASHBOARD_SELF_DEPLOY_OPERATOR_METRICS_ADDR` | `127.0.0.1:19090` | Metrics/health bind address for the local Operator process. |
| `DASHBOARD_SELF_DEPLOY_TARGET_DIR` | `${CARGO_TARGET_DIR}` if set, otherwise `target` | Cargo target directory used to resolve default local binary paths. |
| `DASHBOARD_SELF_DEPLOY_OPERATOR_BIN` | `<target-dir>/debug/reinhardt-cloud-operator` | Override the local Operator binary path. |
| `DASHBOARD_SELF_DEPLOY_CLI_BIN` | `<target-dir>/debug/reinhardt-cloud` | Override the local CLI binary path. |
| `DASHBOARD_SELF_DEPLOY_MANAGE_BIN` | `<target-dir>/debug/manage` | Override the Dashboard `manage` binary used for strict introspection. |
| `DASHBOARD_SELF_DEPLOY_REINHARDT_ENV` | `ci` | `REINHARDT_ENV` used by local Dashboard `manage introspect`. |
| `DASHBOARD_SELF_DEPLOY_CORE_SECRET_KEY` | self-deploy fixture value | `REINHARDT_CORE__SECRET_KEY` used by local Dashboard `manage introspect`. |
| `DASHBOARD_SELF_DEPLOY_JWT_SECRET` | self-deploy fixture value | `REINHARDT_CLOUD_JWT_SECRET` used by local Dashboard `manage introspect`. |
| `DASHBOARD_SELF_DEPLOY_DATABASE_PASSWORD` | `postgres` | `REINHARDT_DATABASE_PASSWORD` used by local Dashboard `manage introspect`. |
| `DASHBOARD_SELF_DEPLOY_KEEP_RESOURCES` | `0` | Set to `1` to keep the namespace after the run. |
| `DASHBOARD_SELF_DEPLOY_INTROSPECT_TIMEOUT_SECONDS` | `30` | Timeout for each CLI `manage introspect` attempt. The harness fails instead of using zero-config fallback. |
| `DASHBOARD_SELF_DEPLOY_ARTIFACT_DIR` | `target/dashboard-self-deploy-e2e/<namespace>` | Failure diagnostics and generated YAML. |
| `DASHBOARD_SELF_DEPLOY_CLUSTER_MODE` | `auto` | `auto`, `existing`, or `create-kind`. `auto` uses a reachable context, then falls back to kind. |
| `DASHBOARD_SELF_DEPLOY_KUBECTL_CONTEXT` | current context | Kubernetes context for `kubectl`. |
| `DASHBOARD_SELF_DEPLOY_KIND_CLUSTER` | inferred from `kind-*` context, otherwise `reinhardt-dashboard-e2e` when kind is created | Explicit `kind` cluster target for creation and `kind load docker-image`. |
| `DASHBOARD_SELF_DEPLOY_E2E_USERNAME` | `e2e-user` | Username seeded inside the deployed Dashboard Pod for authenticated flow checks. |
| `DASHBOARD_SELF_DEPLOY_E2E_PASSWORD` | random per run | Password assigned to the seeded Dashboard user. Empty values and the former public fixture password are rejected. |
| `DASHBOARD_SELF_DEPLOY_E2E_EMAIL` | `e2e@example.test` | Email assigned to the seeded Dashboard user. |
| `DASHBOARD_SELF_DEPLOY_ALLOW_SEED_USER` | set internally | Explicit marker required by `seed-self-deploy-user`; the harness sets it only for the seed command. |
| `DASHBOARD_SELF_DEPLOY_PORT_FORWARD_PORT` | `18080` | Local port used for Dashboard health, login, and authenticated page checks. |
| `DASHBOARD_SELF_DEPLOY_ORIGIN` | `http://127.0.0.1:8000` | Origin/Referer used for server function POSTs. The default matches the CI `OriginGuardMiddleware` allow-list. |

When running the Dashboard development server on a port other than 8000,
set `PORT` to the served port or add the exact origin under
`[cors].allow_origins`. In debug profiles the router augments configured
origins with `http://localhost:$PORT` and `http://127.0.0.1:$PORT` so
same-origin server-function POSTs do not fail OriginGuard validation after
switching to ports such as 8001.

Dashboard startup also loads `.env.<profile>` and `.env` from the Dashboard
crate root before TOML interpolation. For the default local profile, values in
`dashboard/.env.local` such as `PORT=8001` are available without exporting them
manually; variables already present in the shell still take precedence.

On failure the harness writes events, live `Project` YAML, owned resource
YAML, Pod logs, Operator logs, and the generated CRD YAML under the artifact
directory before cleanup. Login responses, cookies, and authenticated page
responses are also preserved there. This harness validates the generated-config
→ `--direct` → Operator reconciliation → authenticated Dashboard contract. The
Dashboard → Agent relay is still limited as described in
[Known limitations](#known-limitations).

## Automated source pipeline smoke harness

For source builds, webhook automation, and preview environment lifecycle, run
the deterministic smoke suite:

```bash
cargo make source-pipeline-e2e
```

The task builds the local Operator binary, applies the `Project` CRD, creates a
temporary namespace, starts a local Operator unless an in-cluster Operator is
already installed, and runs the `source_pipeline_e2e` nextest target. The suite
uses real Kubernetes reconciliation for the source-build and preview paths, and
uses the Dashboard GitHub webhook helpers for deterministic webhook payload and
manifest assertions.

This is intentionally a smoke tier. It verifies that a source `Project`
creates the expected Kaniko `Job`, that the parent `Project` image and
annotations are updated, that GitHub push and pull request payloads map to the
expected pipeline annotations, and that preview create, update, delete, TTL
cleanup, and owner-reference cleanup are reconciled. It does **not** require a
real GitHub push, external Git clone, registry authentication, or a successful
Kaniko image push. Add those as an opt-in full-build tier when CI has a stable
registry and credentials.

Useful overrides:

| Variable | Default | Purpose |
|---|---|---|
| `REINHARDT_CLOUD_SOURCE_PIPELINE_E2E` | set to `1` by `cargo make source-pipeline-e2e` | Enables the cluster-backed tests. Without it, those tests report a skip and return success. |
| `REINHARDT_CLOUD_SOURCE_PIPELINE_E2E_NAMESPACE` | `rc-e2e-<test>-<uuid>` | Reuse or name the test namespace. Existing namespaces are not deleted; only labeled test `Project` resources are removed. |
| `REINHARDT_CLOUD_SOURCE_PIPELINE_E2E_TIMEOUT_SECONDS` | `90` | Wait timeout for CRD/operator reconciliation assertions. |
| `REINHARDT_CLOUD_SOURCE_PIPELINE_E2E_ARTIFACT_DIR` | `target/source-pipeline-e2e/<namespace>` | Operator logs and failure diagnostics. |
| `REINHARDT_CLOUD_SOURCE_PIPELINE_E2E_KEEP_RESOURCES` | `0` | Set to `1` to keep the namespace and artifacts for debugging. |
| `REINHARDT_CLOUD_E2E_OPERATOR_MODE` | `auto` | `auto`, `existing`, `local`, or `skip`. |
| `REINHARDT_CLOUD_E2E_OPERATOR_BIN` | `target/debug/reinhardt-cloud-operator` | Override the local Operator binary path. |

Direct nextest invocation is also supported:

```bash
REINHARDT_CLOUD_SOURCE_PIPELINE_E2E=1 \
REINHARDT_CLOUD_E2E_OPERATOR_BIN=target/debug/reinhardt-cloud-operator \
cargo nextest run --locked -p reinhardt-cloud-integration-tests --test source_pipeline_e2e
```

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

Local dependency services are launched through the Reinhardt framework's
`manage infra` command via the cargo-make tasks defined in the workspace-root
`Makefile.toml`. (`cargo make runserver` from the workspace root also starts
local infrastructure before applying migrations and launching the dev server,
so this step is only needed when you want the infra without launching the
server.)

```bash
cargo make infra-up
cargo run -p reinhardt-cloud-dashboard --bin manage -- infra status
docker ps --filter name=reinhardt-cloud-dashboard-redis
```

`infra-up` resolves the same Reinhardt settings profile used by the Dashboard
process (`REINHARDT_ENV=local` for the cargo-make tasks), provisions matching
local containers, and stores the resolved framework-managed state under
`.reinhardt/`. Until kent8192/reinhardt-web#5213 is fixed, the Dashboard task
also starts a Redis compatibility container because the Dashboard Redis URL is
currently an application-specific top-level setting. Stop the containers with
`cargo make infra-down`, or recreate from a clean state with
`cargo make infra-reset`.

> **Note:** if you have `REINHARDT_ENV` set in your shell (e.g. left over
> from a CI shell where you exported `REINHARDT_ENV=ci`), both `infra-up`
> and `runserver` will resolve `<env>.toml` instead of `local.toml`. Run
> `unset REINHARDT_ENV` before running `infra-up` to use the default
> `local` profile.

## 3. Install the CRD

Resolved by kent8192/reinhardt-cloud#315 — the CRD YAML now ships with the
Helm chart:

```bash
kubectl apply -f charts/reinhardt-cloud-operator/crds/project-crd.yaml
kubectl get crd projects.paas.reinhardt-cloud.dev
```

## 4. Run the Dashboard

The Dashboard is the HTTP API (`:8000`) and gRPC control plane (`:50051`).
It uses the reinhardt framework's `manage` binary (migrations and runserver
come from the framework, not from a crate-local subcommand).

### 4a. Apply migrations

```bash
cargo make migrate
```

### 4b. Start the server

In a dedicated terminal:

```bash
cargo make runserver
```

Required / useful environment variables:

| Variable | Purpose |
|---|---|
| `REINHARDT_CLOUD_REDIS_URL` | Required. Validated at startup by `RedisValidationHook` in `dashboard/src/config/hooks.rs`. |
| `REINHARDT_CLOUD_CONFIG_DIR` | Optional. Overrides the directory scanned for settings TOML. |
| `REINHARDT_EMAIL__BACKEND` | Optional. Use `console` for local registration without Mailpit, or `smtp` with `REINHARDT_EMAIL__HOST` / `REINHARDT_EMAIL__PORT` when testing real delivery. |

The notification WebSocket route is registered by `WebSocketRunserverHook`.
The gRPC server is spawned alongside HTTP by `GrpcRunserverHook`
(`dashboard/src/config/hooks.rs`) and binds to `127.0.0.1:50051` through the
local profile's `[grpc].bind_host` override (`dashboard/settings/local.toml`).

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
  --control-plane-url https://127.0.0.1:50051 \
  --auth-token "$AGENT_AUTH_TOKEN"
```

| Flag / env var | Default | Purpose |
|---|---|---|
| `--control-plane-url` / `CONTROL_PLANE_URL` | `http://127.0.0.1:50051` | Dashboard gRPC endpoint. Must be an HTTPS endpoint for authenticated agent runs. |
| `--cluster-name` / `CLUSTER_NAME` | (required) | Arbitrary label used in streamed events. |
| `--node-name` / `NODE_NAME` | `unknown` | Reported as the node identifier. |
| `--heartbeat-interval` | `30` | Seconds between heartbeats. |
| `--auth-token` / `AUTH_TOKEN` | (required) | JWT for `AgentServiceClient`; the agent sends it as a `Bearer` `Authorization` header. See `crates/reinhardt-cloud-grpc/src/interceptor.rs` for the claims shape. |

`--control-plane-url` must use `https://` so the bearer token is not sent over
plaintext gRPC. Authenticated local testing needs a TLS-terminating endpoint in
front of the local gRPC server.

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

Expected: a `Project` CRD YAML on stdout. No cluster or Dashboard calls.

### 8b. `--direct` (skip Dashboard, apply CRD directly)

```bash
cargo run -p reinhardt-cloud-cli -- deploy \
  --name demo --image nginx:1.27 --replicas 2 --direct
kubectl get project -A -w
```

Expected: the CRD is applied; the Operator reconciles it and creates the
backing Deployment / Service.

### 8c. Default path (via Dashboard)

```bash
cargo run -p reinhardt-cloud-cli -- deploy \
  --name demo --image nginx:1.27 --replicas 2
```

Expected: the Dashboard returns a 2xx for `POST /deployments`. The CLI
includes the generated `Project` YAML in the request body as
`project_yaml`, but the Dashboard does not yet persist or relay that
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
4. **Manual Agent token wiring is still required in this flow.**
   `dashboard/src/apps/clusters/server_fn.rs` returns `ClusterTokenInfo` with
   an `auth_token` during cluster creation. Ensure that token is surfaced to
   the Agent process as `AUTH_TOKEN` for local runs. Tracked in
   kent8192/reinhardt-cloud#361.

Until each item is resolved, treat the "Dashboard-mediated deploy" scenario
(section 8c) as a partial smoke test: HTTP path and payload acceptance only.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `rustls CryptoProvider` panic at Operator/Agent startup | Already patched; rebuild and rerun. See kent8192/reinhardt-cloud#314. |
| `gRPC connection refused` from Agent | Dashboard's `runserver` is not up, or `REINHARDT_CLOUD_REDIS_URL` is unset (server exits at startup). Check the Dashboard terminal. |
| `Cannot connect to Docker daemon` during integration tests | `DOCKER_HOST` is pointing at Podman. `unset DOCKER_HOST` or install Docker Desktop. See `CLAUDE.md` → "Troubleshooting Container Errors". |
| `kubectl apply` fails with `no matches for kind "Project"` | CRD is not installed. Re-run section 3. |
| `cargo run -p reinhardt-cloud-dashboard` rebuilds on every invocation | Use a dedicated terminal per long-running component so incremental compilation stays warm. |

## References

- Architecture overview: `README.md`
- Kubernetes operator patterns: `instructions/KUBERNETES_PATTERNS.md`
- Proto contract for the Agent stream: `crates/reinhardt-cloud-proto/proto/cluster_agent.proto`
- Agent binary source: `crates/reinhardt-cloud-agent/src/main.rs`
- Dashboard gRPC wiring: `dashboard/src/config/grpc.rs`, `dashboard/src/config/hooks.rs`
- CLI deploy logic: `crates/reinhardt-cloud-cli/src/commands/deploy.rs`
- Dashboard cluster endpoints: `dashboard/src/apps/clusters/`
