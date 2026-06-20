# reinhardt-cloud-operator

Kubernetes operator that reconciles `Project` custom resources into Deployments, Services, Ingresses, and feature-driven infrastructure (database, cache, worker, storage) for Reinhardt web applications.

The primary app `Deployment` and `Service` are reconciled only when absent or already controlled by the same `Project`, so the operator does not adopt unrelated same-name Kubernetes resources.

## Quick links

- Full usage guide → [`docs/tools/operator.md`](../../docs/tools/operator.md)
- Helm chart → [`charts/reinhardt-cloud-operator/`](../../charts/reinhardt-cloud-operator/)
- Crate API → [`docs.rs/reinhardt-cloud-operator`](https://docs.rs/reinhardt-cloud-operator)
- End-to-end deployment flow → [`docs/architecture/deployment-flow.md`](../../docs/architecture/deployment-flow.md)

## Minimal installation

```bash
git clone https://github.com/kent8192/reinhardt-cloud.git
cd reinhardt-cloud
helm install reinhardt-cloud-operator charts/reinhardt-cloud-operator \
  --namespace reinhardt-cloud-system --create-namespace \
  -f charts/reinhardt-cloud-operator/values-onprem.yaml
```

(Use `values-aws.yaml` or `values-gcp.yaml` for cloud deployments.) A published Helm repository is not yet available.

## License

BSL-1.1 — see the repository root `LICENSE`.
