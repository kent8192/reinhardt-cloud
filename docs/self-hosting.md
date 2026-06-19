# Self-Hosting the Reinhardt Cloud Dashboard

## Overview

The Reinhardt Cloud Dashboard can be self-hosted by the same operator it
drives, a practice known as *dogfooding*. The dashboard is expressed as a
`Project` custom resource that the Reinhardt Cloud Operator reconciles
into a `Deployment`, `Service`, and (optionally) an autoscaler. Upgrades are
GitOps-driven: every published GitHub release triggers a workflow that
updates the manifest in the cluster with the newly released image tag.

This page covers:

- [Prerequisites](#prerequisites)
- [Bootstrap](#bootstrap)
- [Upgrade flow](#upgrade-flow)
- [Rollback](#rollback)
- [Observability](#observability)
- [Deployment flow architecture](architecture/deployment-flow.md)

The canonical manifests live at
[`manifests/dashboard-rbac.yaml`](../manifests/dashboard-rbac.yaml) and
[`manifests/dashboard-project.yaml`](../manifests/dashboard-project.yaml), and
the workflow at
[`.github/workflows/deploy-dashboard.yml`](../.github/workflows/deploy-dashboard.yml).

## Prerequisites

Before you enable the `Deploy Dashboard` workflow, the following must be in
place:

1. **Operator installed.** The `reinhardt-cloud-operator` Helm chart
   (`charts/reinhardt-cloud-operator`) is deployed into the
   `reinhardt-cloud-system` namespace, and the `projects.paas.reinhardt-cloud.dev`
   CRD (version `v1alpha2`) is registered in the cluster. See the
   [Operator bootstrap](#operator-bootstrap) section below for three
   install paths (GHCR, local kind cluster, cloud overlay).
2. **Dashboard image published.** The dashboard image is built and pushed to
   `ghcr.io/kent8192/reinhardt-cloud-dashboard:<tag>` for every release. The
   image tag must match the GitHub release tag (minus any leading `v`).
   - **Pull secret.** If you push the dashboard or app images to a private registry, see [docs/registry-and-identity.md](registry-and-identity.md) for setup.
3. **Cluster access.** A base64-encoded kubeconfig is stored as the
   `KUBECONFIG` repository secret. Scope it to the minimum permissions
   required to `get`/`create`/`patch` the `project` resource in
   `reinhardt-cloud-system`.
4. **Secrets pre-populated.** A `Secret` named
   `reinhardt-cloud-dashboard-secrets` exists in the
   `reinhardt-cloud-system` namespace with at least these keys:
   - `email-host` — the SMTP host used for outbound dashboard email.

   The operator resolves the `secretRef:<secret>/<key>` values declared in
   `spec.env` against this `Secret` at reconciliation time. Dashboard JWT,
   core secret, database credentials, and Redis URL env vars are generated
   from the typed `auth`, `database`, and `cache` sections.

## Operator bootstrap

The operator image is published to GHCR by the `build-operator-image`
job in the [Build Release Assets](../.github/workflows/build-release-assets.yml)
workflow on every `reinhardt-cloud-operator@v*` release-plz tag. The
chart's default `image.repository` resolves to
`ghcr.io/kent8192/reinhardt-cloud-operator:<Chart.appVersion>`, so a
vanilla `helm install` works against the public registry.

### Production install (GHCR default)

```bash
helm install reinhardt-cloud-operator \
  charts/reinhardt-cloud-operator \
  -n reinhardt-cloud-system --create-namespace
```

The chart's `_helpers.tpl` resolves `image.tag` to `Chart.appVersion`
when `image.tag` is empty (the default). Pin a specific image tag with
`--set image.tag=<semver>` (without a leading `v`, matching the
workflow's tag-stripping convention) if you need to roll back without
bumping the chart.

### Local kind cluster (development / pre-release validation)

```bash
docker build \
  -f crates/reinhardt-cloud-operator/Dockerfile \
  -t reinhardt-cloud-operator:dev .

kind load docker-image reinhardt-cloud-operator:dev --name <cluster>  # cluster name from `kind create cluster --name`

helm install reinhardt-cloud-operator charts/reinhardt-cloud-operator \
  -n reinhardt-cloud-system --create-namespace \
  --set image.repository=reinhardt-cloud-operator \
  --set image.tag=dev \
  --set image.pullPolicy=Never
```

`pullPolicy=Never` ensures kubelet uses the locally loaded image instead
of trying to pull `reinhardt-cloud-operator:dev` from a registry.

### Cloud overlays

The chart ships overlays for managed and on-prem registries:

- `values-gcp.yaml` — GCP Artifact Registry
- `values-aws.yaml` — AWS ECR
- `values-onprem.yaml` — air-gapped or self-hosted private registry

```bash
helm install reinhardt-cloud-operator charts/reinhardt-cloud-operator \
  -n reinhardt-cloud-system --create-namespace \
  -f charts/reinhardt-cloud-operator/values-gcp.yaml   # or values-aws.yaml / values-onprem.yaml
```

Each overlay pins `image.repository` to the platform-specific registry
placeholder; replace the placeholder values (`<region>`, `<project-id>`,
`<account-id>`) before applying.

## Bootstrap

1. Clone the repository and check out the commit you want to deploy.
2. Create the namespace and Secret (once):

   ```bash
   kubectl create namespace reinhardt-cloud-system \
     --dry-run=client -o yaml | kubectl apply -f -

   kubectl -n reinhardt-cloud-system create secret generic \
     reinhardt-cloud-dashboard-secrets \
     --from-literal=email-host='<smtp-provider-host>'
   ```

3. Apply the dashboard RBAC manifest:

   ```bash
   kubectl apply -f manifests/dashboard-rbac.yaml
   ```

4. Render the Project manifest locally and apply it to bootstrap the first
   version:

   ```bash
   VERSION=1.0.0
   sed "s|__VERSION__|${VERSION}|g" manifests/dashboard-project.yaml \
     | kubectl apply -f -
   ```

5. Confirm the operator picked up the resource:

   ```bash
   kubectl -n reinhardt-cloud-system get project reinhardt-cloud-dashboard
   ```

From this point on, upgrades are managed by the workflow described below.

## Upgrade flow

`Deploy Dashboard` (`.github/workflows/deploy-dashboard.yml`) runs in two
modes:

- **Release trigger.** When a GitHub release is published, the workflow
  strips the leading `v` from the tag, substitutes it into the manifest's
  `__VERSION__` placeholder, applies the result with `kubectl apply`, and
  waits up to five minutes for the `Project` to reach the `Ready`
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
   image is currently declared in the `Project` spec, so treat this
   as a temporary measure and follow it with step 1.

## Observability

Inspect the state of the self-hosted dashboard with:

```bash
# High-level status (image, replicas, Ready condition).
kubectl -n reinhardt-cloud-system get project \
  reinhardt-cloud-dashboard

# Full spec and status, including condition history.
kubectl -n reinhardt-cloud-system describe project \
  reinhardt-cloud-dashboard

# Operator logs covering reconciliations of this resource.
kubectl -n reinhardt-cloud-system logs \
  deployment/reinhardt-cloud-operator \
  --tail=200 | grep reinhardt-cloud-dashboard

# Pod-level status for the dashboard itself.
kubectl -n reinhardt-cloud-system get pods \
  -l app.kubernetes.io/name=reinhardt-cloud-dashboard
```

The `Project` status surfaces standard Kubernetes conditions including
`Ready`, `MigrationReady`, `Progressing`, and `Degraded`. If `Ready` or a
database-backed deployment's `MigrationReady` stays `False` for longer than
the workflow's wait timeout, investigate the operator logs before re-running
the workflow.
