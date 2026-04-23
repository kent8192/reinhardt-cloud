# Self-Hosting the Reinhardt Cloud Dashboard

## Overview

The Reinhardt Cloud Dashboard can be self-hosted by the same operator it
drives, a practice known as *dogfooding*. The dashboard is expressed as a
`ReinhardtApp` custom resource that the Reinhardt Cloud Operator reconciles
into a `Deployment`, `Service`, and (optionally) an autoscaler. Upgrades are
GitOps-driven: every published GitHub release triggers a workflow that
updates the manifest in the cluster with the newly released image tag.

This page covers:

- [Prerequisites](#prerequisites)
- [Bootstrap](#bootstrap)
- [Upgrade flow](#upgrade-flow)
- [Rollback](#rollback)
- [Observability](#observability)

The canonical manifest lives at [`manifests/dashboard-app.yaml`](../manifests/dashboard-app.yaml)
and the workflow at
[`.github/workflows/deploy-dashboard.yml`](../.github/workflows/deploy-dashboard.yml).

## Prerequisites

Before you enable the `Deploy Dashboard` workflow, the following must be in
place:

1. **Operator installed.** The `reinhardt-cloud-operator` Helm chart
   (`charts/reinhardt-cloud-operator`) is deployed into the
   `reinhardt-cloud-system` namespace, and the `reinhardtapps.paas.reinhardt-cloud.dev`
   CRD (version `v1alpha2`) is registered in the cluster.
2. **Dashboard image published.** The dashboard image is built and pushed to
   `ghcr.io/kent8192/reinhardt-cloud-dashboard:<tag>` for every release. The
   image tag must match the GitHub release tag (minus any leading `v`).
3. **Cluster access.** A base64-encoded kubeconfig is stored as the
   `KUBECONFIG` repository secret. Scope it to the minimum permissions
   required to `get`/`create`/`patch` the `reinhardtapp` resource in
   `reinhardt-cloud-system`.
4. **Secrets pre-populated.** A `Secret` named
   `reinhardt-cloud-dashboard-secrets` exists in the
   `reinhardt-cloud-system` namespace with at least these keys:
   - `jwt-secret` — the JWT signing secret used by the dashboard.
   - `database-url` — the URL of the dashboard's PostgreSQL database.

   The operator resolves the `secretRef:<secret>/<key>` values declared in
   `spec.env` against this `Secret` at reconciliation time.

## Bootstrap

1. Clone the repository and check out the commit you want to deploy.
2. Create the namespace and Secret (once):

   ```bash
   kubectl create namespace reinhardt-cloud-system \
     --dry-run=client -o yaml | kubectl apply -f -

   kubectl -n reinhardt-cloud-system create secret generic \
     reinhardt-cloud-dashboard-secrets \
     --from-literal=jwt-secret='<random-64-bytes>' \
     --from-literal=database-url='postgres://user:pass@host:5432/dashboard'
   ```

3. Render the manifest locally and apply it to bootstrap the first version:

   ```bash
   VERSION=1.0.0
   sed "s|__VERSION__|${VERSION}|g" manifests/dashboard-app.yaml \
     | kubectl apply -f -
   ```

4. Confirm the operator picked up the resource:

   ```bash
   kubectl -n reinhardt-cloud-system get reinhardtapp reinhardt-cloud-dashboard
   ```

From this point on, upgrades are managed by the workflow described below.

## Upgrade flow

`Deploy Dashboard` (`.github/workflows/deploy-dashboard.yml`) runs in two
modes:

- **Release trigger.** When a GitHub release is published, the workflow
  strips the leading `v` from the tag, substitutes it into the manifest's
  `__VERSION__` placeholder, applies the result with `kubectl apply`, and
  waits up to five minutes for the `ReinhardtApp` to reach the `Ready`
  condition.
- **Manual dispatch.** The same workflow can be run manually from the
  Actions tab. Provide the desired image tag in the `version` input (for
  example `1.2.3`) to pin a specific revision without cutting a release.

The operator then performs a rolling update on the owned `Deployment` —
the dashboard is effectively redeploying itself through its own reconciler.

## Rollback

There are two supported ways to roll back:

1. **Re-run the workflow with the previous tag.** Trigger the `Deploy
   Dashboard` workflow via `workflow_dispatch` and supply the prior version
   as the `version` input. The operator reconciles the image change with
   the same rolling-update semantics used on upgrade.
2. **Roll back the owned Deployment directly.** For emergency rollbacks
   without waiting for a workflow run:

   ```bash
   kubectl -n reinhardt-cloud-system rollout undo \
     deployment/reinhardt-cloud-dashboard
   ```

   Note that on the next reconcile the operator will re-apply whatever
   image is currently declared in the `ReinhardtApp` spec, so treat this
   as a temporary measure and follow it with step 1.

## Observability

Inspect the state of the self-hosted dashboard with:

```bash
# High-level status (image, replicas, Ready condition).
kubectl -n reinhardt-cloud-system get reinhardtapp \
  reinhardt-cloud-dashboard

# Full spec and status, including condition history.
kubectl -n reinhardt-cloud-system describe reinhardtapp \
  reinhardt-cloud-dashboard

# Operator logs covering reconciliations of this resource.
kubectl -n reinhardt-cloud-system logs \
  deployment/reinhardt-cloud-operator \
  --tail=200 | grep reinhardt-cloud-dashboard

# Pod-level status for the dashboard itself.
kubectl -n reinhardt-cloud-system get pods \
  -l app.kubernetes.io/name=reinhardt-cloud-dashboard
```

The `ReinhardtApp` status surfaces standard Kubernetes conditions
(`Ready`, `Progressing`, `Degraded`). If `Ready` stays `False` for longer
than the workflow's wait timeout, investigate the operator logs before
re-running the workflow.
