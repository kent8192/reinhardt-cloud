# reinhardt-cloud CLI

Command-line tool for deploying and inspecting Reinhardt applications.

## Commands

- `deploy` — deploy a Reinhardt application using zero-config inference or explicit config.
- `status` — show the current status of a deployed application.

## Distributed Tracing

The CLI exports OpenTelemetry spans when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset (tracing disabled) | OTLP gRPC endpoint |
| `OTEL_SERVICE_NAME` | `reinhardt-cloud-cli` | Service name in spans |
| `OTEL_TRACES_SAMPLER` | `parentbased_traceidratio` | Sampler strategy |
| `OTEL_TRACES_SAMPLER_ARG` | `0.1` | Sampling ratio (0.0–1.0) |

### Root spans

| Span | Command | Attributes |
|------|---------|------------|
| `cli.deploy` | `deploy` | `app.name`, `app.namespace`, `api.version` |
| `cli.status` | `status` | `app.name`, `app.namespace`, `api.version` |

### End-to-end trace propagation

The operator reads annotation `reinhardt.io/traceparent` on a `ReinhardtApp` to stitch an
incoming distributed trace into its `operator.reconcile` span. Writing that annotation is the
responsibility of the caller — a future CLI release will set it automatically during `deploy` so
that the CLI span and the downstream operator span appear in a single distributed trace. This
feature is not yet implemented in the current phase.
