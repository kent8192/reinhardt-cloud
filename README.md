<div align="center">
  <img src="branding/logo.png" alt="Reinhardt Cloud Logo" width="200"/>

  <h1>Reinhardt Cloud</h1>

  <h3>Convention-driven deployment for Reinhardt apps</h3>

  <p><strong>A Kubernetes-native PaaS</strong> — deploy
  <a href="https://github.com/kent8192/reinhardt-web">Reinhardt</a>
  web applications with zero infrastructure configuration.</p>
  <p>Named after Django Reinhardt's composition <em>Nuages</em> (French: "Clouds").</p>

[![CI](https://github.com/kent8192/reinhardt-cloud/actions/workflows/ci.yml/badge.svg)](https://github.com/kent8192/reinhardt-cloud/actions/workflows/ci.yml)
[![Security Audit](https://github.com/kent8192/reinhardt-cloud/actions/workflows/security-audit.yml/badge.svg)](https://github.com/kent8192/reinhardt-cloud/actions/workflows/security-audit.yml)
[![codecov](https://codecov.io/gh/kent8192/reinhardt-cloud/graph/badge.svg)](https://codecov.io/gh/kent8192/reinhardt-cloud)
[![License](https://img.shields.io/badge/license-BSL--1.1-blue.svg)](LICENSE)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/kent8192/reinhardt-cloud)

</div>

---

## Quick Navigation

- [Who is Reinhardt Cloud For?](#who-is-reinhardt-cloud-for)
- [Quick Start](#quick-start)
- [Why Reinhardt Cloud?](#why-reinhardt-cloud)
- [Architecture](#architecture)
- [Key Features](#key-features)
- [CLI Reference](#cli-reference)
- [CRD Reference](#crd-reference)
- [Installation (Operator)](#installation)
- [Configuration](#configuration)
- [Workspace Crates](#workspace-crates)
- [Development](#development)
- [API Stability](#api-stability)
- [Self-hosting](#self-hosting)

## Who is Reinhardt Cloud For?

**For App Developers** who:

- Build [Reinhardt](https://github.com/kent8192/reinhardt-web) web applications and want `git push`-style deployment
- Want automatic infrastructure provisioning (database, cache, storage) based on your app's feature flags
- Prefer convention over configuration for Kubernetes — no hand-written YAML

**For Platform Operators** who:

- Run Kubernetes clusters and want a PaaS layer for your team's Reinhardt apps
- Need multi-cloud support (AWS, GCP, on-prem) with Helm-based installation
- Want CRD-driven, GitOps-compatible application management

## Quick Start

> **Status:** v0.1.0 pre-release. CLI commands are functional but under active development.

### 1. Initialize from an existing Reinhardt project

```bash
cd my-project
reinhardt-cloud init        # Detects project structure, generates reinhardt-cloud.toml
```

This produces a `reinhardt-cloud.toml` based on your project's Cargo features and settings:

```toml
[app]
name = "my-app"
image = "my-app:latest"

[database]
engine = "postgresql"
storage_gb = 20

[health]
path = "/api/healthz/"
port = 8000
interval_seconds = 10

[services]
port = 80
target_port = 8000
ingress_host = "app.example.com"

[services.tls]
enabled = true
secret_name = "app-example-com-tls"
issuer = "letsencrypt-ns"

[scale]
min_replicas = 2
max_replicas = 6
metric = "cpu"
target_value = 70
```

### 2. Preview and deploy

```bash
reinhardt-cloud deploy --dry-run   # Preview the generated Project CRD as YAML
reinhardt-cloud login --token rct_example
reinhardt-cloud deploy --cluster production  # Submit through the Dashboard
```

### 3. Check status

```bash
reinhardt-cloud status --name my-app
```

## Why Reinhardt Cloud?

Deploying a Reinhardt web application to Kubernetes typically means writing Deployments, Services, StatefulSets, Ingresses, and more — even though the framework already knows what the app needs.

Reinhardt Cloud takes a different approach: **convention-driven deployment**. The CLI runs `manage introspect` against your Reinhardt project, detects its feature flags (database, auth, cache, pages, etc.), and generates a single `Project` CRD. The operator reconciles that CRD into real Kubernetes resources.

| Inspiration | What We Borrowed | What We Added |
|---|---|---|
| **Vercel** | Three-plane architecture (CLI, Control Plane, Runtime) | Kubernetes-native, self-hosted |
| **Heroku** | Convention-driven deployment | CRD-based, GitOps-compatible |
| **Crossplane** | Composition Functions pattern | Reinhardt-specific inference |
| **Django `manage.py`** | Introspection-based tooling | Automatic infrastructure detection |

**Result**: A platform where `reinhardt-cloud deploy` is all you need — the framework tells the platform what infrastructure to provision.

## Architecture

Three-plane architecture inspired by Vercel:

```mermaid
C4Container
    title Reinhardt Cloud - Three-Plane Architecture

    Person(dev, "Developer", "Builds Reinhardt web applications")

    Container_Boundary(cli_plane, "CLI Plane") {
        Container(cli, "reinhardt-cloud CLI", "Rust, clap", "Analyzes projects via manage introspect and generates Project CRDs")
    }

    Container_Boundary(cp_plane, "Control Plane") {
        Container(dashboard, "Dashboard", "Rust, reinhardt-web", "Pages UI, server functions, authentication, project management")
        ContainerDb(pg, "PostgreSQL", "", "Users, projects, deployments")
    }

    Container_Boundary(k8s_plane, "Kubernetes Cluster") {
        Container(operator, "Operator", "Rust, kube-rs", "Reconciles Project CRDs into Deployments, Services, StatefulSets, Ingress, HPA")
        Container(agent, "Agent", "Rust, tonic", "Bidirectional gRPC streaming with control plane")
        ContainerDb(crd, "Project CRD", "v1alpha2", "Desired application state")
    }

    Rel(dev, cli, "Uses")
    Rel(cli, dashboard, "deploy", "HTTPS")
    Rel(cli, crd, "dry-run / direct", "kubectl apply")
    Rel(dashboard, pg, "Reads/Writes", "SQL")
    Rel(dashboard, agent, "Commands", "gRPC")
    Rel(agent, operator, "Reports status", "gRPC streaming")
    Rel(operator, crd, "Watches and reconciles")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

| Plane | Crate | Role |
|---|---|---|
| **CLI** | `reinhardt-cloud-cli` | Developer-facing tool. Analyzes projects via `manage introspect`, generates CRDs, communicates with the control plane. |
| **Control Plane** | `dashboard` | A [reinhardt-web](https://github.com/kent8192/reinhardt-web) application providing a Pages UI, server functions, authentication, and project management. |
| **Operator** | `reinhardt-cloud-operator` | Kubernetes controller that watches `Project` CRDs and reconciles them into infrastructure resources. |

**Supporting services:**

- **Agent** (`reinhardt-cloud-agent`) — Bidirectional gRPC communication between control plane and clusters.
- **gRPC layer** (`reinhardt-cloud-proto`, `reinhardt-cloud-grpc`) — Four gRPC services across five proto files: Agent, Build, Log, Plugin (plus Common shared types).

For the end-to-end deployment flow — CLI branches, dashboard relay, agent behaviour, and reconciler output — see [`docs/architecture/deployment-flow.md`](docs/architecture/deployment-flow.md).

## Key Features

- **Convention-Driven Deployment** — CLI introspects your Reinhardt project and infers infrastructure needs from Cargo feature flags and settings
- **Project CRD** — Single custom resource (`paas.reinhardt-cloud.dev/v1alpha2`) that declares your entire application stack
- **Automatic Infrastructure** — PostgreSQL/MySQL database, Redis cache, S3/GCS/PVC object storage, SMTP mail, background workers
- **Autoscaling** — HPA-based scaling on CPU, memory, or requests-per-second with configurable thresholds
- **Workload Isolation** — gVisor, Kata Containers, network policies (Cilium), seccomp profiles, Pod Security Standards
- **Multi-Tenant Namespacing** — `TenantRef` on the CRD maps each app to an Organization/Team and enforces a deterministic, isolated namespace with per-tenant `ResourceQuota` and `NetworkPolicy`
- **Dashboard Authentication** — Local credentials plus GitHub OAuth, account-page linking, logout, and email-verification flow
- **Preview Environments** — Per-PR ephemeral deployments with TTL, templated ingress hostnames, and override-able replica/database/cache settings
- **Crossplane-style Plugins** — `PluginSpec` extension points reconciled via the gRPC Plugin service (Composition Functions pattern)
- **Private Registry & Workload Identity** — `image_pull_secrets` and per-app `ServiceAccount` for IRSA / Workload Identity Federation
- **Multi-Cloud Helm Charts** — AWS, GCP, and on-prem values out of the box
- **`reinhardt-cloud.toml`** — Human-readable project configuration that maps 1:1 to the CRD spec
- **`manage introspect` Integration** — Detects databases, routes, middleware, and feature flags from your Reinhardt project
- **Reinhardt Pages Support** — Automatic static asset serving configuration for WASM+SSR frontends
- **gRPC Microservices** — Build streaming, log ingestion/tailing, agent orchestration, and Crossplane-style plugin functions
- **Deletion Policy** — Retain or Delete cloud resources on app teardown

## CLI Reference

For the complete guide — every command, flag, example, persona-specific note, and troubleshooting entry — see [`docs/tools/cli.md`](docs/tools/cli.md).

Summary:

```
reinhardt-cloud [--server <URL>] <command>
```

| Command | Description |
|---|---|
| `init` | Generate `reinhardt-cloud.toml` from project analysis |
| `sync` | Re-synchronize `reinhardt-cloud.toml` with current project state |
| `deploy` | Build the `Project` CRD and submit it through the Dashboard, or apply it directly with `--direct` |
| `status` | Check deployment status |
| `login` | Verify and persist a Dashboard API token |
| `credentials` | Manage Git and container-registry credentials |
| `crd` | Generate CRD manifests for GitOps workflows |

See [`docs/tools/cli.md`](docs/tools/cli.md) for flags, subcommand details, examples, and troubleshooting.

## CRD Reference

The `Project` custom resource is the single source of truth for your application's desired state.

```yaml
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  name: my-app
  namespace: default
spec:
  image: my-app:v1
  replicas: 3
  database:
    engine: Postgresql
    storage_gb: 20
    version: "16"
  cache:
    backend: Redis
  auth:
    jwt: true
  scale:
    min_replicas: 1
    max_replicas: 10
    metric: Cpu
    target_value: 80
  health:
    path: /healthz
    port: 8080
  services:
    port: 80
    target_port: 8080
    ingress_host: myapp.example.com
  deletion_policy: Retain
```

### Spec fields

| Field | Type | Description |
|---|---|---|
| `image` | `String` | Docker image to deploy (required) |
| `replicas` | `i32?` | Number of replicas (default: 1) |
| `database` | `DatabaseSpec?` | PostgreSQL / MySQL provisioning |
| `cache` | `CacheSpec?` | Redis cache |
| `worker` | `WorkerSpec?` | Background worker processes |
| `auth` | `AuthSpec?` | JWT + OAuth configuration |
| `storage` | `StorageSpec?` | S3 / GCS / PVC object storage |
| `mail` | `MailSpec?` | SMTP configuration |
| `scale` | `ScaleSpec?` | HPA autoscaling for CPU and Memory; RPS is reserved for custom metrics |
| `health` | `HealthSpec?` | Liveness / readiness probes |
| `services` | `ServicesSpec?` | Port + Ingress exposure. Generated Ingress hosts must match a DNS suffix configured by `REINHARDT_CLOUD_INGRESS_HOST_SUFFIXES` and be unique across the cluster; the operator rejects `services.ingress_host` values outside those suffixes or already claimed by another Ingress. |
| `services.tls` | `ServiceTlsSpec?` | Ingress TLS settings: `enabled`, `secret_name`, `issuer`; `cluster_issuer` is rejected for tenant safety |
| `pages` | `PagesSpec?` | WASM+SSR static asset config |
| `isolation` | `IsolationSpec?` | Runtime class, network policy, seccomp |
| `deletion_policy` | `DeletionPolicy` | `Retain` (default) or `Delete` |
| `features` | `Vec<String>` | Resolved reinhardt-web feature flags |
| `env` | `BTreeMap<String, String>` | Environment variables |
| `introspect` | `IntrospectOutput?` | Metadata from `manage introspect` |
| `source` | `SourceSpec?` | Git repository, build settings, and PR-based preview environments |
| `tenant` | `TenantRef?` | Owning Organization (and optional Team) for multi-tenant namespacing |
| `plugins` | `Vec<PluginSpec>?` | Crossplane-style Composition Functions for extending the reconciler |
| `image_pull_secrets` | `Vec<LocalObjectReference>?` | Private container-registry pull secrets |
| `service_account` | `ServiceAccountSpec?` | Per-app `ServiceAccount` for IRSA / Workload Identity Federation |

### Status conditions

The operator reports the following conditions on the CRD status:

`Ready`, `Progressing`, `Degraded`, `MigrationReady`, `DatabaseReady`, `CacheReady`, `WorkerReady`, `IngressReady`, `TlsReady`, `AutoscalerReady`

For database-backed projects, the operator runs a revision-scoped migration
Job before applying the new workload `Deployment`. Migration Jobs inherit the
workload runtime class, service account, plugin mounts, resource defaults, and
isolated workload security contexts, and isolation resources are reconciled
before the Job is created. `MigrationReady=False` with reason
`MigrationRunning` means rollout is waiting on that Job; `MigrationReady=False`
with reason `MigrationFailed` blocks the rollout and marks the project degraded
until the spec changes or the failed revision is handled.

Autoscaling uses Kubernetes `autoscaling/v2` HPA for `cpu` and `memory`.
`min_replicas` and `max_replicas` must be at least `1`. For `memory`,
`target_value` is MiB. `rps` is reserved for custom/external metrics and
surfaces `AutoscalerReady=False` until a custom metrics provider is supported.

The `[scale]` example above generates an HPA like:

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: my-app
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: my-app
  minReplicas: 2
  maxReplicas: 6
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
```

For `metric = "memory"` with `target_value = 512`, the generated resource
target uses `type: AverageValue` and `averageValue: 512Mi`.

For source-driven deployments, source builds populate `status.build` with the active or most
recent Kaniko build. `status.build.jobName`, `status.build.trigger`, `status.build.image`, and
`status.build.imageTag` identify the build Job and produced image. Production `spec.image`
updates and preview Project image updates are applied only after the associated Kaniko Job
succeeds.

## Installation

### Prerequisites

- Kubernetes 1.31+
- Helm 3
- **cert-manager** (required for preview environments) — the operator emits a
  per-namespace `cert-manager.io/v1` `Issuer` for each parent-qualified preview namespace so preview hosts get automatic TLS. Configure the issuer with:
  - `REINHARDT_CLOUD_PREVIEW_INGRESS_CLASS` (default `nginx`)
  - `REINHARDT_CLOUD_PREVIEW_ACME_SERVER` (default Let's Encrypt production)
  - `REINHARDT_CLOUD_PREVIEW_ACME_EMAIL` (registration email; set in production)

### Install the operator

```bash
helm install reinhardt-cloud-operator ./charts/reinhardt-cloud-operator \
  --namespace reinhardt-cloud-system \
  --create-namespace
```

### Cloud-specific installations

```bash
# AWS
helm install reinhardt-cloud-operator ./charts/reinhardt-cloud-operator \
  -f charts/reinhardt-cloud-operator/values-aws.yaml \
  --namespace reinhardt-cloud-system --create-namespace

# GCP
helm install reinhardt-cloud-operator ./charts/reinhardt-cloud-operator \
  -f charts/reinhardt-cloud-operator/values-gcp.yaml \
  --namespace reinhardt-cloud-system --create-namespace

# On-prem
helm install reinhardt-cloud-operator ./charts/reinhardt-cloud-operator \
  -f charts/reinhardt-cloud-operator/values-onprem.yaml \
  --namespace reinhardt-cloud-system --create-namespace
```

### Feature toggles

Enable or disable infrastructure components in your Helm values:

```yaml
features:
  database: true
  cache: false
  ingress: false
  autoscaling: false
  storage: false
  worker: false
```

### Isolation defaults

The operator ships with sensible security defaults:

```yaml
isolation:
  defaultLevel: "None"
  networkPolicy:
    enabled: true
    provider: cilium
    blockMetadataService: true
  podSecurityStandards:
    enabled: true
    enforceLevel: restricted
  seccomp:
    enabled: true
    profile: RuntimeDefault
```

> See `charts/reinhardt-cloud-operator/values.yaml` for all isolation settings including runtime classes, resource limits, and egress rules.

## Configuration

The `reinhardt-cloud.toml` file is the human-readable project configuration. It maps 1:1 to the `Project` CRD spec.

Generate it automatically:

```bash
reinhardt-cloud init    # from your Reinhardt project directory
```

### Full example

```toml
[app]
name = "my-app"
image = "my-app:v2"

[database]
engine = "postgresql"
instance_class = "db.t3.micro"
storage_gb = 50
version = "16"

[auth]
jwt = true

[health]
path = "/health"
port = 3000
interval_seconds = 15

[services]
port = 443
target_port = 3000
ingress_host = "app.example.com"

[services.tls]
enabled = true
secret_name = "app-example-com-tls"
issuer = "letsencrypt-ns"

[replicas]
count = 3

[scale]
min_replicas = 2
max_replicas = 20
metric = "cpu"
target_value = 80

[cache]
backend = "redis"

[worker]
concurrency = 8

[storage]
backend = "s3"
bucket = "my-bucket"

[env]
CUSTOM_VAR = "custom_value"
```

## Workspace Crates

| Crate | Type | Description |
|---|---|---|
| `reinhardt-cloud` | Library | Facade crate that re-exports public library components |
| `reinhardt-cloud-types` | Library | CRD types, config schema, validation, introspect types |
| `reinhardt-cloud-core` | Library | Business logic, plugin system, auth, pagination |
| `reinhardt-cloud-k8s` | Library | Kubernetes client helpers and resource builders |
| `reinhardt-cloud-proto` | Library | Protocol Buffers definitions (5 services) |
| `reinhardt-cloud-grpc` | Library | gRPC client/server implementations, SSE adapter |
| `reinhardt-cloud-operator` | Binary | Kubernetes operator (reconciler, resource management) |
| `reinhardt-cloud-cli` | Binary | `reinhardt-cloud` command-line tool |
| `reinhardt-cloud-agent` | Binary | Cluster agent for bidirectional control plane communication |
| `dashboard` | Application | Control Plane web app ([reinhardt-web](https://github.com/kent8192/reinhardt-web)) |
| `tests` | Integration Tests | Cross-crate integration test suite |

### gRPC services

| Proto | Service | Description |
|---|---|---|
| `cluster_agent.proto` | `AgentService` | Bidirectional streaming between control plane and cluster agents |
| `build.proto` | `BuildService` | Build lifecycle management with log streaming |
| `log.proto` | `LogService` | Log ingestion (client streaming) and tailing (server streaming) |
| `plugin.proto` | `PluginService` | Crossplane Composition Functions pattern for extensibility |
| `common.proto` | — | Shared pagination and status types |

## Development

### Prerequisites

- Rust (2024 Edition)
- Docker (required for TestContainers — not Podman)
- cargo-make, cargo-nextest

For a step-by-step local stack bootstrap (cluster + Dashboard + Operator + Agent + end-to-end deploy), see [`docs/development/LOCAL_E2E_TESTING.md`](docs/development/LOCAL_E2E_TESTING.md).

### Commands

```bash
# Build
cargo check --workspace --all-features
cargo build --workspace --all-features

# Test
cargo make test                                 # all tests
cargo nextest run --workspace --all-features    # with nextest

# Code quality
cargo make fmt-check
cargo make clippy-check
cargo make clippy-todo-check    # detect TODO/FIXME

# Full pre-PR check
cargo make pre-pr

# Run the dashboard (Control Plane)
cargo make runserver

# Run the operator locally
cargo run --bin reinhardt-cloud-operator
```

## API Stability

**Current status:** v0.1.0 (Alpha)

| Component | Stability | Notes |
|---|---|---|
| `Project` CRD (`v1alpha2`) | Alpha | Schema may change |
| CLI commands | Alpha | Flags and behavior may change |
| gRPC services | Alpha | Protobuf schema may change |
| Helm chart | Alpha | Values structure may change |
| `reinhardt-cloud.toml` | Alpha | Keys and format may change |

Breaking changes will be documented in release notes.

## Self-hosting

The Reinhardt Cloud Dashboard can be self-hosted through its own operator
as a `Project`. A canonical manifest (`manifests/dashboard-project.yaml`)
and a release-triggered deploy workflow
(`.github/workflows/deploy-dashboard.yml`) implement this GitOps-driven
dogfooding flow. See [docs/self-hosting.md](docs/self-hosting.md) for
bootstrap, upgrade, rollback, and observability instructions.
For private registry access and cloud workload identity, see
[docs/registry-and-identity.md](docs/registry-and-identity.md).

## Getting Help

- [GitHub Discussions](https://github.com/kent8192/reinhardt-cloud/discussions) — Ask questions and share ideas
- [GitHub Issues](https://github.com/kent8192/reinhardt-cloud/issues) — Report bugs
- [Security Policy](SECURITY.md) — Report vulnerabilities

## Contributing

We welcome contributions! See the [Development](#development) section to set up your environment.

**Quick links:**
- [Pull Request Template](.github/PULL_REQUEST_TEMPLATE.md)
- [GitHub Issues](https://github.com/kent8192/reinhardt-cloud/issues)

## Star History

<a href="https://star-history.com/#kent8192/reinhardt-cloud&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=kent8192/reinhardt-cloud&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=kent8192/reinhardt-cloud&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=kent8192/reinhardt-cloud&type=Date" width="600" />
 </picture>
</a>

## Copyright

Copyright &copy; 2026 Tachyon Inc. All rights reserved.

Developed by Tachyon Inc.

## License

This project is licensed under the [Business Source License 1.1](LICENSE).
