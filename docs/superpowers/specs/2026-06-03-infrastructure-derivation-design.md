# Infrastructure Derivation Design

## Context

Issue #625 tracks the missing link between Reinhardt application settings and
per-application Terraform resources. `ReinhardtAppSpec.infrastructure` already
exists as the explicit input consumed by `reinhardt-cloud terraform generate`,
but `deploy` currently attaches introspection metadata without deriving that
infrastructure block. As a result, applications that declare database or
storage requirements still need hand-authored Terraform input before managed
per-app resources can be generated.

The design target is an explicit, reviewable baseline infrastructure spec. The
operator must not become the component that mutates or backfills Terraform
authoring input.

## Goals

- Derive a baseline `spec.infrastructure` from introspection and typed app
  configuration.
- Keep Terraform generation driven by explicit CRD spec whenever possible.
- Provide a small compatibility fallback for CRDs that have introspection but no
  infrastructure block.
- Fail early when derivation would produce ambiguous or unsupported cloud
  resources.
- Preserve explicit infrastructure values and never overwrite user-refined
  fields.

## Non-Goals

- Do not make the operator patch `spec.infrastructure`.
- Avoid inferring undeclared secrets from naming conventions or environment names.
- Not intended to support every possible backend in the first iteration.
- Will not silently map local or PVC storage to managed cloud buckets.

## Recommended Approach

Use the A plus small C fallback approach:

1. Primary path: `deploy` and `sync` derive and persist a baseline
   `InfrastructureSpec`.
2. Compatibility path: `terraform generate` may derive from `spec.introspect`
   only when `spec.infrastructure` is absent, and must warn that the generated
   block should be persisted.
3. The operator remains out of the authoring path. It continues using
   introspection only for in-cluster reconciliation decisions.

This keeps the CRD spec as the visible contract for Terraform while still
allowing older manifests to produce useful output.

## Component Boundaries

### `reinhardt-cloud-core`

Add a pure derivation module that accepts stable inputs and returns either a
valid `InfrastructureSpec` or a structured derivation error.

The core module should own:

- database engine support checks,
- storage backend support checks,
- default Postgres values,
- stable generated bucket names,
- validation before returning a spec,
- merge behavior that preserves explicit infrastructure.

The module should not read files, inspect the Kubernetes cluster, shell out, or
print warnings.

### `reinhardt-cloud-cli deploy`

When building `ReinhardtAppSpec`, `deploy` should:

1. read `reinhardt-cloud.toml` as it does today,
2. collect real or synthesized introspection,
3. preserve any explicit infrastructure from the TOML-derived spec,
4. derive baseline infrastructure only when the spec has no infrastructure
   block,
5. fail the command if derivation returns an error.

Dry-run output should include the derived infrastructure block so the generated
manifest can be reviewed before apply.

### `reinhardt-cloud-cli sync`

`sync` should update `reinhardt-cloud.toml` with an infrastructure section when
supported signals are present. That file becomes the normal refinement point for
managed cloud resources.

Existing explicit infrastructure values should be preserved. The generated
baseline should only fill missing sections.

### `reinhardt-cloud-cli terraform generate`

`terraform generate` should prefer explicit infrastructure. If no
infrastructure block is available but introspection is available, it may derive
infrastructure with the same core function and emit a warning.

The fallback is for migration and convenience only. It must not become a second
independent inference implementation.

### Operator

The operator should not patch or default `spec.infrastructure`. Runtime
reconcile decisions may continue using existing introspection inference for
database, cache, worker, storage, mail, pages, and ingress resources.

## Derivation Rules

### Postgres

Derive `InfrastructureSpec.postgres` only when the effective database signal is
`postgres` or `postgresql`.

Default values:

- `version`: `16`
- `backup_retention_days`: `7`
- `tier`: leave unset unless an explicit infrastructure value or typed config
  provides one

Unsupported database engines must fail early when managed infrastructure
derivation is requested. MySQL and SQLite should not be silently ignored or
mapped to Postgres.

### Buckets

Derive one bucket when the effective storage signal is `s3` or `gcs`.

The default bucket name should be stable and app-scoped, for example:

```text
<app-name>-assets
```

The generated name must pass `BucketSpec` validation. Unsupported storage
signals such as `local`, `pvc`, or unknown strings must fail early instead of
being mapped to a cloud bucket.

### Secrets

Only derive secrets from typed, explicit references already available to
`reinhardt-cloud-types`, such as source credential refs, webhook secret refs, or
mail credential refs. Do not infer secrets from arbitrary environment variable
names or from the current introspection schema, because the schema does not
declare application secrets directly.

If there are no typed secret references, leave `secrets` unset.

### DNS

Do not derive DNS records from routes alone. Routes show HTTP paths, not owned
hostnames. DNS derivation is out of scope for issue #625. A later issue can add
DNS derivation from explicit hostname configuration once the infrastructure TOML
schema has a dedicated representation for DNS records.

## Fail-Early Policy

Derivation must fail before emitting manifests or Terraform when it encounters
unsupported managed infrastructure inputs.

Required early errors:

- unsupported database engine for managed database derivation,
- unsupported storage backend for managed bucket derivation,
- invalid generated bucket or secret names,
- invalid merged `InfrastructureSpec`,
- conflicting explicit and derived values when they cannot be merged without
  losing information.

The CLI should report actionable errors that name the offending signal and the
supported values. It should not downgrade these cases to warnings.

## Merge Policy

Explicit values always win.

When an explicit `InfrastructureSpec` exists, derivation should only fill missing
top-level sections if that behavior is deliberately enabled by the caller.
`deploy` should be conservative and avoid modifying an explicit infrastructure
block unless `sync` or a future dedicated command performs a merge intended for
editing.

For `sync`, merging is acceptable because the command's purpose is to refresh
generated configuration. Even there, existing section values should be preserved
field by field.

## Error Handling

Core errors should be structured enough for tests and CLI messages:

- unsupported database engine,
- unsupported storage backend,
- invalid derived resource,
- conflicting explicit infrastructure.

CLI commands should convert these errors to concise human-readable messages and
exit non-zero.

## Testing

Core tests:

- derives Postgres from `postgres`,
- derives Postgres from `postgresql`,
- rejects MySQL and SQLite for managed Postgres derivation,
- derives a bucket from `s3`,
- derives a bucket from `gcs`,
- rejects `local`, `pvc`, and unknown storage backends,
- leaves secrets unset when no typed secret refs exist,
- preserves explicit infrastructure.

CLI tests:

- `deploy --dry-run` includes derived infrastructure when introspection has
  supported signals,
- `deploy --dry-run` preserves explicit infrastructure,
- `deploy --dry-run` exits with an error on unsupported managed signals,
- `sync` writes an infrastructure section without clobbering explicit values,
- `terraform generate` uses explicit infrastructure when present,
- `terraform generate` uses the introspection fallback only when infrastructure
  is absent and emits a warning,
- `terraform generate` exits early on fallback derivation errors.

## Documentation

Update CLI documentation to explain:

- `spec.infrastructure` is the Terraform input contract,
- `deploy` and `sync` can generate a baseline infrastructure block,
- unsupported managed backends fail early,
- Terraform's introspection fallback is a migration path, not the preferred
  workflow.

## Open Implementation Notes

The first implementation should add the derivation function and CLI wiring in a
small sequence:

1. Add core derivation types and tests.
2. Add TOML schema support for infrastructure.
3. Wire `deploy` derivation.
4. Wire `sync` persistence.
5. Add the `terraform generate` fallback.
6. Update CLI docs.
