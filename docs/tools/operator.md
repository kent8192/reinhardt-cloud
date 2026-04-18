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

## Distributed Tracing

The operator exports OpenTelemetry spans when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.
Via the Helm chart this is controlled by `tracing.enabled=true` and `tracing.endpoint`.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset (tracing disabled) | OTLP gRPC endpoint |
| `OTEL_SERVICE_NAME` | `reinhardt-cloud-operator` | Service name in spans |
| `OTEL_TRACES_SAMPLER` | `parentbased_traceidratio` | Sampler strategy |
| `OTEL_TRACES_SAMPLER_ARG` | `0.1` | Sampling ratio (0.0–1.0) |

### Span names

| Span | Description |
|------|-------------|
| `operator.reconcile` | Root span per `ReinhardtApp` reconcile pass |

Span attributes: `resource.kind`, `resource.namespace`, `resource.name`, `reconcile_id`.

### CRD annotation contract

When a caller sets annotation `reinhardt.io/traceparent` on a `ReinhardtApp`, the operator reads it as the parent context and stitches its `operator.reconcile` span into the caller's distributed trace.

Writing the annotation back is deferred to avoid patch-loop reconcile storms.

### Trace-to-log correlation

When running with `REINHARDT_LOG_FORMAT=json`, structured log lines include `trace_id` and `span_id` fields sourced from the active OTel span. Filter logs by `trace_id` in Loki/Grafana to correlate logs with traces.

### Managed Pod trace propagation

The operator injects the following env vars into each managed Pod's container spec:
- `TRACEPARENT` — W3C trace context header from the active reconcile span.
- `OTEL_PROPAGATORS=tracecontext` — instructs OTel SDKs to read `TRACEPARENT`.
- `OTEL_SERVICE_NAME` — the app name.
- `OTEL_EXPORTER_OTLP_ENDPOINT` — forwarded from the operator's env when set.
