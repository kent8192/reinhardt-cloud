# Tool Usage Guides

> **Last verified**: commit `84d08ad` on 2026-04-18

This directory contains detailed usage guides for each tool that ships with Reinhardt Cloud. If you are looking for architecture or development documentation, see the repository root `README.md`.

## Choose your starting point

### If you are an App Developer

1. [CLI](cli.md) — `init`, `deploy`, `status`, and friends
2. [Dashboard](dashboard.md) — inspect your apps in the browser
3. [Operator: For App Developers](operator.md#for-app-developers) — what runs behind the scenes

### If you are a Platform Operator

1. [Operator](operator.md) — installation, upgrade, operations
2. [Agent](agent.md) — per-cluster enrollment
3. [Dashboard: Deployment](dashboard.md#deployment-of-the-dashboard-itself-for-platform-operators)
4. [CLI: Platform-Ops notes](cli.md) — callouts scattered through command sections

## Tool Selection Matrix

| Task | CLI | Dashboard | Operator | Agent |
|---|---|---|---|---|
| Bootstrap a new project | ✅ `init` | — | — | — |
| Resynchronize config with features | ✅ `sync` | — | — | — |
| One-off deploy to a cluster | ✅ `deploy` | ✅ (Apps → Deploy) | — (reconciles CRD) | — (applies on cluster) |
| GitOps deploy (no direct apply) | ✅ `crd generate` | — | ✅ (watches CRDs) | ✅ (applies on cluster) |
| Multi-cluster fan-out | — | ✅ | — | ✅ (per-cluster) |
| Check app health | ✅ `status` | ✅ (Apps list) | source of truth | report source |
| View application logs | — | ✅ (Logs viewer) | — | streams to Dashboard |
| Manage Git/registry credentials | ✅ `credentials` | partially (Settings) | — | — |
| Install / upgrade the platform | — | — | ✅ (Helm) | ✅ (Helm, per cluster) |
| Authenticate users | ✅ `login` | ✅ | — | — |

## How to keep these docs fresh

Each file begins with `Last verified: <commit> on <date>`. When you modify a tool's public surface (CLI flag, CRD field, Dashboard route, Agent RPC), update the matching file and bump the header.

If you find drift, file an Issue on `kent8192/reinhardt-cloud` with label `documentation`.

## Cross-references

- Repository root README: `../../README.md`
- CRD manifests: `../../charts/reinhardt-cloud-operator/crds/`
- Helm chart values: `../../charts/reinhardt-cloud-operator/values.yaml` (comments are authoritative)
