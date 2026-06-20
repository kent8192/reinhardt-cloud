# reinhardt-cloud Dashboard

> **Last verified**: commit `84d08ad` on 2026-04-18
> **Source of truth**: this file. `dashboard/README.md` is a summary (updated in a later task to link here).
> **Audience**: this guide serves both App Developers (first half) and Platform Operators (Deployment section onward); persona callouts inside feature subsections highlight when guidance diverges.

## Overview

The reinhardt-cloud Dashboard is a full-stack web application that provides a browser-based interface for inspecting and managing `Project` deployments on the Reinhardt Cloud platform. It is the control-plane host: it serves the HTTP API consumed by the CLI, runs the gRPC server that cluster agents connect to, streams build and log output, and presents all of that as an interactive single-page application.

**Runtime stack summary**

- **Backend framework**: reinhardt-web (`reinhardt` workspace crate, native feature set including `reinhardt::url_patterns`, `reinhardt::ServerRouter`) — source: `dashboard/Cargo.toml` native dependencies
- **ORM and migrations**: reinhardt's built-in ORM (`reinhardt::db`) with Rust-source migration files under `dashboard/migrations/` — no raw SQL files, no sqlx/diesel/sea-orm dependencies declared
- **Database engine**: PostgreSQL (inferred from `FieldType::TimestampTz`, `FieldType::Uuid` in migration source, and `engine = "postgresql"` in `dashboard/settings/base.toml`)
- **Frontend framework**: reinhardt-pages + reinhardt-admin features of the `reinhardt` crate — not Yew, Leptos, or Dioxus
- **WASM build**: The library crate declares `crate-type = ["cdylib", "rlib"]` for dual native/WASM compilation. The `dashboard/index.html` file is the WASM shell HTML; `build.rs` uses `cfg_aliases` to set `cfg(wasm)` / `cfg(native)` compile-time flags. WASM bundle build tooling is not declared in `Cargo.toml` — see §7.8 of the source audit (outstanding verification: WASM bundler invocation, e.g. `trunk build`, is not documented in Makefile.toml and must be verified from the broader build pipeline before documenting the exact command)
- **gRPC**: tonic 0.13 + tonic-reflection 0.13, hosting `AgentService` and `BuildService` on port 50051 by default (`crates/reinhardt-cloud-grpc/src/config.rs`)
- **Settings loader**: environment-variable-driven TOML files in `dashboard/settings/` (selected via `REINHARDT_SETTINGS_MODULE`)

### Dashboard vs CLI vs Operator

| Tool | Primary user | Strength |
|------|-------------|----------|
| Dashboard | App Developers and Platform Operators | Browser GUI; real-time log streaming; deployment history; cluster management |
| CLI (`reinhardt-cloud`) | App Developers | Scripting, CI/CD pipelines, terminal-first workflows |
| Operator | Platform Operators (Kubernetes admins) | In-cluster reconciliation; CRD lifecycle management; no human in the loop |

### Runtime stack

| Layer | Technology | Source reference |
|-------|-----------|-----------------|
| Backend web framework | reinhardt (native features) | `dashboard/Cargo.toml` `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` |
| ORM | reinhardt::db | `dashboard/migrations/auth/0001_initial.rs` `use reinhardt::db::migrations::prelude::*` |
| DB engine (production) | PostgreSQL | `dashboard/settings/base.toml` `engine = "postgresql"` |
| Frontend framework | reinhardt pages + admin | `dashboard/Cargo.toml` WASM deps `reinhardt = { features = ["pages", "admin"] }` |
| WASM build tooling | Outstanding verification (see §7.8) | `dashboard/index.html` exists; bundler not confirmed |
| gRPC server | tonic 0.13, default port 50051 | `crates/reinhardt-cloud-grpc/src/config.rs` line 25 |
| Migration format | Rust source files | `dashboard/migrations/` (no `.sql` files) |

---

## Getting Started (For Users)

### Accessing the Dashboard

There are two common ways to reach the Dashboard:

**Option 1 — Via an Ingress hostname (production / shared environments)**

The platform operator publishes the Dashboard behind a Kubernetes `Ingress` resource. Ask your operator for the assigned hostname. No local setup is needed.

**Option 2 — `kubectl port-forward` (local / development access)**

The Dashboard HTTP server listens on port 8000 by default (reinhardt-web framework default; confirmed by `cors.allow_origins = ["http://localhost:8000", ...]` in `dashboard/settings/base.toml`). To forward that port from a running Dashboard pod:

```bash
kubectl port-forward -n <namespace> deployment/<dashboard-deployment-name> 8000:8000
```

Then open `http://localhost:8000` in a browser.

> **Note for Platform Operators**: the exact Deployment name depends on your install manifest. There is no Helm chart for the Dashboard today (see [Installation options](#installation-options)).

### First login

The Dashboard supports credential-based authentication and configured GitHub OAuth. Visit `/login` for local username/password sign-in. After signing in, use `/account` to view the current profile, link a GitHub account, or log out from the dashboard shell.

### Layout tour

The WASM client router (`dashboard/src/client/router.rs`) registers the top-level client routes, and the HTTP server mounts project namespaces plus an admin panel:

1. **Dashboard shell** (`/`) — the root application shell; landing view after login
2. **Account** (`/account`) — profile summary and GitHub account linking
3. **Auth** (`/auth/`) — login, registration, OAuth callback, and session endpoints
4. **Clusters** (`/clusters/`) — registered Kubernetes cluster list and management
5. **Deployments** (`/deployments/`) — deployment records paired with operator `Project` CRDs
6. **Admin panel** (`/api/admin/`) — operator-level administration UI (reinhardt-admin)

---

## Features

### Apps list

The **Deployments** section (`/deployments/`) presents the PaaS-side records that correspond to `Project` CRDs in the cluster. Each entry shows the project name, the associated cluster, and deployment metadata recorded by the Dashboard when a deploy was triggered via the CLI or directly through the API.

The **Clusters** section (`/clusters/`) shows registered Kubernetes clusters (cluster-management records stored in the Dashboard's own database, not the operator's CRD list).

Dashboard operation forms use inventory-backed selectors for cluster, repository, and deployment targets. Operators choose records by recognizable names and metadata; the form posts the corresponding persisted ID internally.

> **For App Developers**: after running `reinhardt-cloud deploy`, navigate to `/deployments/` and locate your application by name. The record should appear within seconds. Cross-check the `Ready` condition by also running `reinhardt-cloud status --name <app>` from the terminal.

> **For Platform Operators**: use the `/api/admin/` panel to list all deployments across all users. The `DeploymentAdmin` registered in `dashboard/src/config/admin.rs` exposes the full deployment table. Filter by cluster or by creation date to identify stale or failing entries.

### Deployment details and history

The deployments application (`dashboard/src/apps/deployments/`) maintains a database record for each deploy event. The detail view shows the image, replica count, and the cluster the deploy was sent to.

Rollback capability via the Dashboard UI is not confirmed in source. To roll back a running workload, use `reinhardt-cloud deploy` with an older image tag, or apply the desired `Project` spec directly with `reinhardt-cloud deploy --direct`.

### Logs viewer

Application logs are read through the Dashboard's JWT-protected gRPC `LogServiceServer`. In development the server is backed by the in-process `LocalLogService`; in clusters it can be backed by `reinhardt-cloud-telemetry::LokiLogService` by setting `log_backend = "loki"` or `REINHARDT_CLOUD_LOG_BACKEND=loki`. The Loki backend reads historical logs with `/loki/api/v1/query_range` and tails live logs with `/loki/api/v1/tail`.

Managed application pods are written to Loki by the Helm chart's Promtail app scrape job when `logging.scrapeApps=true`. The Dashboard resolves the selected `deployment_id` through the current user's organization, enforces `LogsRead`, then maps the deployment's project name to the Loki `app` label, the deployment primary key to the Loki `deployment_id` label, and the organization slug to the deterministic tenant namespace. Historical log requests use `deployment_logs_for_current_org`; live tailing uses the authenticated `/ws/notifications` WebSocket and sends `SubscribeAppLogs { deployment_id }`. Both paths include the tenant namespace in their gRPC log filter so project-name collisions in other organizations do not match the same Loki query.

Direct gRPC log reads must include a non-empty `source` or `deployment_id` filter so backend queries cannot default to a cross-application log selector. Filters available in the generic log service depend on `crates/reinhardt-cloud-proto/proto/log.proto` and `reinhardt_cloud_types::log::LogFilter`: `source` (project/app label), `min_level`, `since`, `until`, `search`, `deployment_id`, and `namespace`. The Dashboard managed-app log viewer sets `source`, `deployment_id`, and the tenant `namespace` after authorization so shared log backends constrain results to the selected deployment instead of relying on a user-controlled project name alone.

> **Security note**: Logs may contain personally identifiable information if applications log request bodies, headers, or user input. The Dashboard streams log content without server-side masking or redaction. App Developers should ensure their applications do not log sensitive data at `INFO` level or above.

### Metrics and status

The operator exposes custom Prometheus metrics on `/metrics` when chart metrics are enabled:

- `reinhardt_cloud_operator_reconcile_total{result}`
- `reinhardt_cloud_operator_reconcile_duration_seconds{result}`
- `reinhardt_cloud_operator_requeue_total{reason}`
- `reinhardt_cloud_operator_managed_apps{phase}`
- `reinhardt_cloud_operator_managed_apps_ready_replicas{namespace,project}`
- `reinhardt_cloud_operator_managed_apps_desired_replicas{namespace,project}`

Dashboard status views can combine these operator metrics with kube-state-metrics and agent-reported deployment status from `AgentService.ReportDeployStatus` / `AgentService.ReportHealth`.

### Settings

The Dashboard exposes an admin panel at `/api/admin/` powered by reinhardt-admin. Three model types are registered (`dashboard/src/config/admin.rs`):

- **User** — `UserAdmin` (auth app)
- **Cluster** — `ClusterAdmin` (clusters app)
- **Deployment** — `DeploymentAdmin` (deployments app)

There is no `settings` application module in `dashboard/src/apps/` at this commit. User-facing profile management and API token management are handled via the `auth` app (`/auth/`).

> **For Platform Operators**: the `/api/admin/` panel requires the account to have staff/admin access as configured through the reinhardt-admin framework. User activation and deactivation can be managed from the User admin list. There is no formal per-tenant quota UI today.

---

## Deployment of the Dashboard Itself (For Platform Operators)

### Installation options

There is no Helm chart for the Dashboard. The `charts/` directory contains only `charts/reinhardt-cloud-operator/`. Deploy the Dashboard by applying the canonical `Project` manifest in `manifests/dashboard-project.yaml`.

The published Dashboard image contains two runtime binaries:

1. `/app/reinhardt-cloud-dashboard` — production server entrypoint for the main container
2. `/app/manage` — management command binary used by operator init containers for migrations and static collection

The image also:

1. Sets `REINHARDT_SETTINGS_MODULE=reinhardt_cloud_dashboard.config.settings` at process startup
2. Bundles Dashboard settings under `/app/settings`
3. Exposes port 8000 for the HTTP API

### Database requirements

- **ORM**: reinhardt::db (built-in ORM from the `reinhardt` crate)
- **Supported engines**: PostgreSQL — the only engine declared in `dashboard/settings/base.toml` (`engine = "postgresql"`)
- **Migration source**: `dashboard/migrations/` — four app-level sub-directories (`auth/`, `clusters/`, `deployments/`, `default/`); all migrations are Rust source files
- **Migration tooling**: run via the `manage` binary:

```bash
cargo run --bin manage migrate
# or with cargo-make:
cargo make migrate
```

The migration command is provided by reinhardt-web's built-in `migrate` management command (invoked through `execute_from_command_line()` in `dashboard/src/bin/manage.rs`).

### Static asset / WASM asset caching

The Dashboard serves admin static assets at `/api/static/admin/` via reinhardt-admin's built-in static file serving. The WASM bundle for the client SPA is loaded by `dashboard/index.html` (700 B shell HTML at repo root of the dashboard directory).

**Outstanding verification**: the exact path from which the backend serves the compiled WASM `.wasm` and `.js` glue files (e.g. a `dist/` or `staticfiles/` directory) is not confirmed at this commit — `dashboard/build.rs` configures `cfg_aliases` only; no `trunk build` invocation is visible in `Makefile.toml`. Until this is confirmed, cache headers and versioning strategy for the WASM bundle cannot be documented authoritatively. Operators should configure their reverse proxy (nginx, ALB) to set a short `Cache-Control` max-age (e.g. 60 seconds) on `/api/static/admin/` paths until the WASM serving path is documented.

### Configuration via `dashboard/settings/`

The Dashboard uses reinhardt-web's standard TOML settings loader — not a Rust `#[settings]`-decorated struct tree. There are no `SettingsFragment` or `settings_toml!` macro calls in the codebase. Configuration is split into per-environment TOML files:

| File | Purpose |
|------|---------|
| `dashboard/settings/base.toml` | Shared settings across all environments |
| `dashboard/settings/local.toml` | Development overrides |
| `dashboard/settings/ci.toml` | CI-specific overrides |
| `dashboard/settings/staging.toml` | Staging overrides |
| `dashboard/settings/production.toml` | Production overrides |

The active settings module is selected at startup via:

```bash
REINHARDT_SETTINGS_MODULE=reinhardt_cloud_dashboard.config.settings
```

This is set unconditionally by `dashboard/src/bin/manage.rs` before delegating to `execute_from_command_line()`.

**Representative configuration keys** (from `dashboard/settings/base.toml`):

| Key | Description |
|-----|-------------|
| `core.debug` | Enable debug mode (`false` in production) |
| `core.secret_key` | HMAC signing key — must be changed from the placeholder |
| `core.allowed_hosts` | List of permitted `Host` header values |
| `core.databases.default.engine` | DB backend (`"postgresql"`) |
| `core.databases.default.host` | DB hostname |
| `core.databases.default.port` | DB port (default `5432`) |
| `core.databases.default.name` | Database name |
| `core.databases.default.user` | DB username |
| `core.databases.default.password` | DB password — use a secret reference in production |
| `core.security.secure_ssl_redirect` | Redirect HTTP to HTTPS (`false` by default) |
| `cors.allow_origins` | Allowed CORS origins for the `OriginGuardMiddleware` |
| `static_files.url` | URL prefix for static files (default `/static/`) |
| `static_files.root` | Filesystem path to static files |

Override at deploy time by mounting a `local.toml` as a `ConfigMap` volume, or by supplying a `production.toml` file alongside `base.toml` in the settings directory. The reinhardt-web loader merges files in lexicographic order of their names.

### GitHub OAuth

GitHub OAuth is enabled when all required provider settings and the OAuth token encryption key are present in the runtime environment. The login and registration pages show only configured providers. Existing users can link GitHub from `/account`; the callback attaches the provider identity to the active session user when a valid `sessionid` cookie is present.

The dashboard persists GitHub OAuth access tokens only after encrypting them with `REINHARDT_CLOUD_OAUTH_TOKEN_ENCRYPTION_KEY`. Set this variable to a base64-encoded 32-byte key before enabling GitHub OAuth. The stored token is used to verify GitHub App setup callbacks against `/user/installations`; OAuth storage APIs still return tokenless account records to normal authentication callers.

Generate a local development key with:

```bash
openssl rand -base64 32
```

Set `REINHARDT_CLOUD_GITHUB_APP_INSTALL_URL` to the GitHub App installation URL shown in GitHub App settings. The GitHub repository page uses it for the empty-state install action after the current user has linked a GitHub OAuth account. The dashboard appends a signed, short-lived `state` parameter to this URL and requires `OrgUpdate` permission before accepting the `/api/github/setup/` callback, so only organization administrators can bind a GitHub App installation to organization state.

### Operations

#### Backup

The Dashboard's only stateful component is its PostgreSQL database. Back it up with:

```bash
pg_dump -h <host> -U <user> -d <dbname> -Fc -f dashboard_$(date +%Y%m%d).dump
```

Restore with:

```bash
pg_restore -h <host> -U <user> -d <dbname> -Fc dashboard_<date>.dump
```

There are no additional stateful volumes to back up at this commit (no uploaded media, no local file storage declared in settings).

#### Upgrade order

When upgrading the Dashboard to a new version that includes schema changes:

1. Run migrations first (with the old binary or a migration-only init container):
   ```bash
   cargo run --bin manage migrate
   ```
2. Roll the Dashboard Deployment pods to the new image.

This order prevents the new code from running against an un-migrated schema.

#### Multi-tenancy

The Dashboard is multi-tenant at the application layer: every Cluster and Deployment row carries an `organization_id` foreign key, and every authenticated user has at least one `OrganizationMembership` (auto-provisioned as a "Personal Organization" on registration). Cross-organization access is filtered at every read query and refused with HTTP 403 at every write request — see Appendix B for the permission matrix.

---

## Troubleshooting

### WASM bundle fails to load (blank page or console MIME type error)

**Symptom**: The browser loads `index.html` but the application never renders. The DevTools console shows a MIME type error or a 404 for the `.wasm` file.

**Cause**: The WASM bundle is not present at the path the server is configured to serve, or the web server is serving the `.wasm` file with `Content-Type: text/html` instead of `application/wasm`.

**Fix**: Verify that the WASM assets have been compiled and placed in the expected static directory. Ensure your reverse proxy or CDN serves `.wasm` files with `Content-Type: application/wasm`. Clear browser cache and retry.

### Cannot log in (credentials rejected or 500 error)

**Symptom**: Submitting the login form at `/login` returns an error.

- If the error message is "invalid credentials": double-check the username and password. Use the admin panel (`/api/admin/`) to verify the user exists and is active.
- If the page returns a 500 error: the Dashboard cannot reach its database. Check that `core.databases.default` in `base.toml` points to a running PostgreSQL instance. Check Dashboard pod logs:
  ```bash
  kubectl logs -n <namespace> deployment/<dashboard> --tail=50
  ```

### Logs not streaming

**Symptom**: The logs viewer shows no output or immediately disconnects.

**Cause**: The cluster agent is not connected to the Dashboard's gRPC server, or the WebSocket connection from the browser to the Dashboard is being blocked.

**Fix**:
1. Verify the agent is running and connected: see [agent.md troubleshooting](agent.md#troubleshooting).
2. Confirm the gRPC server is listening on port 50051 inside the Dashboard pod.
3. Confirm your ingress/load balancer allows WebSocket upgrades (`Connection: Upgrade`, `Upgrade: websocket` headers must be forwarded).

### Dashboard returns 500 on every request (DB migration not applied)

**Symptom**: All Dashboard pages return a 500 Internal Server Error immediately after a version upgrade.

**Cause**: The new binary expects schema changes that have not been applied.

**Fix**: Run migrations before rolling the Deployment:

```bash
cargo run --bin manage migrate
```

Then redeploy (or restart the pods) to pick up the migrated schema.

### App shows as Ready in Dashboard but pod is in CrashLoopBackOff

**Symptom**: The Dashboard reports the application as `Ready`, but the actual pod is failing.

**Cause**: The Dashboard's deployment record reflects the last known status reported by the cluster agent. Agent status updates are asynchronous; a brief lag is expected.

**Fix**: Check the actual pod state directly:

```bash
reinhardt-cloud status --name <app> --namespace <ns>
kubectl describe pod -n <ns> -l app.kubernetes.io/name=<app>
kubectl logs -n <ns> -l app.kubernetes.io/name=<app> --previous
```

If the discrepancy persists beyond a few minutes, verify the agent's heartbeat is being received (check Dashboard logs for `AgentHeartbeat` entries).

---

## Appendix A: Route reference

| Path | Handler | Notes |
|------|---------|-------|
| `/` | Dashboard shell (WASM SPA entrypoint) | Client-side router takes over after initial load |
| `/account` | Account page | Shows profile, GitHub linking state, and logout control |
| `/login` | Login page | WASM client route; auth POST goes to `/auth/` API |
| `/register` | Registration page | WASM client route |
| `/auth/` | Auth app URL patterns | Login, registration, OAuth, and session API endpoints |
| `/clusters/` | Clusters app URL patterns | Cluster CRUD API |
| `/deployments/` | Deployments app URL patterns | Deployment record API |
| `/api/admin/` | reinhardt-admin panel | Requires admin account |
| `/api/static/admin/` | Admin static files | Served by reinhardt-admin |

Source: `dashboard/src/config/urls.rs` and `dashboard/src/client/router.rs`.

---

## Appendix B: Permission model

The Dashboard enforces a 4-role RBAC matrix at the view boundary. Membership rows carry one of the following roles, stored as a lowercase string with a database `CHECK` constraint:

| Role        | Hierarchy | Capability summary                                                                |
|-------------|-----------|-----------------------------------------------------------------------------------|
| `owner`     | highest   | Org-level superuser. The only role allowed to delete the organization itself.     |
| `admin`     |           | Manage members, manage all clusters and deployments. Cannot delete the org.       |
| `developer` |           | Create/read/update/delete clusters and deployments. Cannot manage members.        |
| `viewer`    | lowest    | Read-only access to clusters, deployments, and logs across the organization.      |

Every authenticated request enters the matrix through `apps::organizations::permissions::require_permission(user_id, action)`, which:

1. Resolves the user's active organization via the `OrganizationMembership` table.
2. Looks up the user's role in that organization.
3. Consults the static `(role, action)` matrix in `apps::organizations::permissions::table::allowed`.
4. Returns the resolved `organization_id` on allow, or `AppError::Authorization` (HTTP 403) on deny.

The full action catalog (`OrgRead`, `OrgUpdate`, `OrgDelete`, `MemberInvite`, `MemberRemove`, `MemberChangeRole`, `ClusterCreate`, `ClusterRead`, `ClusterUpdate`, `ClusterDelete`, `DeploymentCreate`, `DeploymentRead`, `DeploymentUpdate`, `DeploymentDelete`, `LogsRead`) is defined in `apps::organizations::permissions::action::Action`. Adding a new action requires:

1. Adding the variant to `Action`.
2. Extending the `match` in `permissions::table::allowed` with a row per role.
3. Adding the `(role, action)` rows to `tests/unit/test_permissions.rs`.

The `match` is exhaustive, so omitting any role for the new action is a compile error.

The reinhardt-admin `/api/admin/` panel is still gated separately by the `is_staff` / `is_superuser` flags on the `User` model and is not part of the organization-scoped RBAC matrix.

---

## Appendix C: Audit log format

Audit logging is not implemented in the Dashboard at this commit. Application-level events (logins, deploys triggered via API) are not written to a structured audit log. The Dashboard does emit structured `tracing` logs to stdout (inheriting the reinhardt-web framework's tracing integration), but these are operational logs, not a security audit trail.
