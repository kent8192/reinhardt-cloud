# Tool Usage Guides

The workspace ships the CLI, Kubernetes operator, cluster agent, and shared
libraries, plus a Pages Dashboard described in the repository README with PostgreSQL
sessions, organization-scoped project reads, and transactional runtime intent
storage. Agent delivery and deployment mutation APIs are not connected; CLI
commands that need those APIs require a separately provided compatible server.

## App Developers

1. [CLI](cli.md) — initialize projects, generate manifests, and deploy with `--direct`.
2. [Operator: For App Developers](operator.md#for-app-developers) — infrastructure reconciliation.

## Platform Operators

1. [Operator](operator.md) — installation, upgrades, and operations.
2. [Agent](agent.md) — commands and telemetry for an external compatible control plane.
3. [CLI](cli.md) — configuration and manifest generation.

## Tool Selection

| Task | CLI | Operator | Agent |
|---|---|---|---|
| Bootstrap a project | `init` | — | — |
| Resynchronize configuration | `sync` | — | — |
| Deploy directly to a cluster | `deploy --direct` | Reconciles the Project CRD | — |
| GitOps deployment | Generate a Project manifest | Watches Project CRDs | — |
| Check deployment health | `status` with kubeconfig | Writes status conditions | Reports to an external control plane |
| Install the platform | — | Helm | Helm, with an external control plane |

See the repository README for workspace development commands.
