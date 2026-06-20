# reinhardt-cloud-agent

Per-cluster agent that connects a Kubernetes cluster to an external Reinhardt Cloud control plane via bidirectional gRPC streaming. Enables multi-cluster fleet management, log streaming to the Dashboard, and health reporting.

## Quick links

- Full usage guide → [`docs/tools/agent.md`](../../docs/tools/agent.md)
- Crate API → [`docs.rs/reinhardt-cloud-agent`](https://docs.rs/reinhardt-cloud-agent)

## When you need it

The agent is optional. Install it when:
- You manage multiple Kubernetes clusters from a single Dashboard, or
- You want the Dashboard's Logs viewer to stream pod logs, or
- The control plane cannot reach the cluster's kube-apiserver directly (NAT, firewall, air-gap).

Single-cluster deployments driven by the CLI or GitOps do NOT need the agent.

## Minimal installation (raw manifest)

An agent Helm chart is not yet packaged. For now:

```bash
kubectl create namespace reinhardt-cloud-system
kubectl -n reinhardt-cloud-system create secret generic reinhardt-cloud-agent-token \
  --from-literal=token=<control-plane-issued-token>

# Apply a Deployment referencing the Secret and an HTTPS control-plane URL.
# The agent refuses plaintext control-plane URLs.
# See docs/tools/agent.md for the full manifest.
```

See the [full guide](../../docs/tools/agent.md) for enrollment flow, RBAC, and troubleshooting.

The agent requires `AUTH_TOKEN` and attaches it to the control-plane stream as
`Authorization: Bearer <token>`. Legacy direct Deploy commands are rejected;
workload rollout changes should be sent as Project manifests so the operator
performs validation and reconciles the Kubernetes resources.

## License

BSL-1.1 — see the repository root `LICENSE`.
