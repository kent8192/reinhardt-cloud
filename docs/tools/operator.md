# reinhardt-cloud-operator

The operator watches `ReinhardtApp` custom resources and reconciles them into
Kubernetes workloads. See the Helm chart at `charts/reinhardt-cloud-operator/`.

## Structured Logging

The operator emits structured JSON logs when `REINHARDT_LOG_FORMAT=json` is set
in its environment. Via the Helm chart this is controlled by
`logging.format=json`.

Each JSON line conforms to the `LogRecord` schema exposed by the
[`reinhardt-cloud-telemetry`](../../crates/reinhardt-cloud-telemetry) crate:

| Field | Type | Description |
|-------|------|-------------|
| `ts` | RFC3339 UTC | Event timestamp |
| `level` | `trace` / `debug` / `info` / `warn` / `error` | Log level |
| `msg` | string | Message body |
| `reconcile_id` | string, optional | Correlates all logs within a single reconcile pass |
| `deployment_id` | string, optional | Managed `ReinhardtApp` deployment identifier |
| `resource_kind` / `resource_namespace` / `resource_name` | string, optional | Target Kubernetes resource |
| `phase` | string, optional | Reconcile phase (e.g. `apply`, `finalize`) |
| `correlation_id` | string, optional | Cross-component correlation (CLI -> operator) |
| `trace_id` / `span_id` | string, optional | Populated automatically once Phase 3 (Issue #374) lands |

### Shipping logs to Loki

Set `logging.loki.enabled=true` in the Helm chart to deploy a Promtail
DaemonSet that tails operator pods and forwards logs to
`logging.loki.endpoint` (default `http://loki.monitoring.svc.cluster.local:3100`).

The operator pod does **not** embed a Loki client — the write path is
out-of-process so that operator pod restarts never block on log-ingest.

### Programmatic access

`reinhardt-cloud-telemetry` exposes the `LogService` trait with two
implementations:

- `InMemoryLogService` — bounded ring buffer + broadcast fan-out for live
  tail. Default: 1000 records, 1-hour TTL.
- `LokiLogService` — read-oriented stub pointing at a Loki instance
  (writes are expected to flow through the DaemonSet above).

The dashboard (Issue #371) will consume `LogService.tail` for live log
streaming.
