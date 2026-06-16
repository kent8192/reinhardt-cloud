# reinhardt-cloud-agent Helm chart

Installs the Reinhardt Cloud Agent as a Deployment in a target cluster. The
Agent observes workload state in its namespace and reports it to the
Reinhardt Cloud control plane (Dashboard). For the control-plane operator
that manages `Project` CRDs, see
[charts/reinhardt-cloud-operator](../reinhardt-cloud-operator).

## Install

Using an out-of-band Secret (recommended):

```bash
kubectl create secret generic reinhardt-cloud-agent-auth \
  --from-literal=token='<control-plane-token>' \
  -n reinhardt-cloud-agent

helm install reinhardt-cloud-agent charts/reinhardt-cloud-agent \
  -n reinhardt-cloud-agent --create-namespace \
  --set controlPlane.url=https://dashboard.example.com \
  --set controlPlane.clusterName=prod-cluster \
  --set auth.existingSecret=reinhardt-cloud-agent-auth
```

Dev-only shortcut (chart manages the Secret):

```bash
helm install reinhardt-cloud-agent charts/reinhardt-cloud-agent \
  -n reinhardt-cloud-agent --create-namespace \
  --set controlPlane.url=http://host.docker.internal:8000 \
  --set controlPlane.clusterName=local \
  --set auth.createSecret=true \
  --set auth.token='dev-token'
```

## Values

| Key | Default | Description |
|-----|---------|-------------|
| `replicaCount` | `1` | Number of Agent pod replicas. |
| `image.repository` | `ghcr.io/kent8192/reinhardt-cloud-agent` | Container image repository. |
| `image.tag` | `""` | Image tag. Empty falls back to `.Chart.AppVersion`. |
| `image.pullPolicy` | `IfNotPresent` | Image pull policy. |
| `imagePullSecrets` | `[]` | List of image pull secret references. |
| `nameOverride` | `""` | Override chart name portion of resource names. |
| `fullnameOverride` | `""` | Override full resource name. |
| `serviceAccount.create` | `true` | Create a ServiceAccount for the Agent. |
| `serviceAccount.annotations` | `{}` | Annotations on the ServiceAccount. |
| `serviceAccount.name` | `""` | Name of the ServiceAccount when not creating. |
| `rbac.create` | `true` | Create namespace-scoped Role/RoleBinding. |
| `controlPlane.url` | `https://dashboard.example.com` | Control-plane base URL. **Required.** |
| `controlPlane.clusterName` | `""` | Logical cluster name. **Required.** |
| `auth.existingSecret` | `""` | Name of existing Secret holding the bearer token. |
| `auth.secretKey` | `token` | Key within the Secret holding the token. |
| `auth.createSecret` | `false` | Render a chart-managed Secret. Dev-only. |
| `auth.token` | `""` | Token value when `createSecret` is true. Dev-only. |
| `resources.requests.cpu` | `100m` | CPU request. |
| `resources.requests.memory` | `128Mi` | Memory request. |
| `resources.limits.cpu` | `500m` | CPU limit. |
| `resources.limits.memory` | `512Mi` | Memory limit. |
| `livenessProbe` | `{}` | Liveness probe. Disabled by default; the Agent does not yet expose an HTTP health endpoint. |
| `readinessProbe` | `{}` | Readiness probe. Disabled by default. |
| `nodeSelector` | `{}` | Node selector. |
| `tolerations` | `[]` | Tolerations. |
| `affinity` | `{}` | Affinity rules. |
| `podAnnotations` | `{}` | Annotations applied to the pod template. |
| `podSecurityContext.runAsNonRoot` | `true` | Refuse to run as UID 0. |
| `podSecurityContext.runAsUser` | `65532` | Non-root UID (distroless-compatible). |
| `securityContext.allowPrivilegeEscalation` | `false` | Disallow privilege escalation. |
| `securityContext.readOnlyRootFilesystem` | `true` | Enforce a read-only root filesystem. |
| `securityContext.capabilities.drop` | `[ALL]` | Drop all Linux capabilities. |

## RBAC

The chart provisions a namespace-scoped Role granting read access to
`apps/Deployments` and `apps/ReplicaSets`. This follows the least-privilege
principle documented in
[`instructions/KUBERNETES_PATTERNS.md`](../../instructions/KUBERNETES_PATTERNS.md)
RB-1. The chart does **not** create a `ClusterRole`.

## Uninstall

```bash
helm uninstall reinhardt-cloud-agent -n reinhardt-cloud-agent
```
