# CLAUDE.md

## Purpose

These instructions apply to `dashboard/`, the Reinhardt Cloud Control Plane: a
Reinhardt Pages application (native server, `manage` binary, WASM frontend).
The repository-level `CLAUDE.md` still applies; this file adds the rules that
are specific to this application. Vocabulary (Control Plane, Dashboard, Agent
Gateway, Project, Deployment, Member, Login Link, ...) follows the glossary in
`CONTEXT.md` on the rebuild integration branch.

## Naming

| Item | Name |
|------|------|
| Cargo package, server binary | `reinhardt-cloud-dashboard` |
| Library crate (`use cloud_control_plane::...`) | `cloud_control_plane` |
| Management binary | `manage` |

The package name is part of the image layout and release tooling. The library
name is the one `reinhardt-admin startproject` generated; do not rename either.

## Project Map

- `src/lib.rs` shared library entry point and server-only macro support.
- `src/server.rs` production HTTP server bootstrap used by the server binary.
- `src/main.rs` server binary (`reinhardt-cloud-dashboard`), a thin launcher.
- `src/bin/manage.rs` management binary (`manage`).
- `src/client/` browser-only WASM launcher.
- `src/apps/` one module per application (below).
- `src/config/` settings, installed apps, project route aggregation, the admin
  site, the `manage` command registry (`config/commands.rs`), and the
  request-surface middleware (`config/middleware/`, `config/web.rs`).
  `routes()` must stay synchronous: an `async` `#[routes]` function registers no
  client routes.
- `src/components/` components shared by every application; `src/i18n/` the
  message catalogs; `src/logging.rs` the process-wide `tracing` output (audit
  events are discarded without it).
- `static/` design tokens, base styles, utilities, and images served through
  `collectstatic`.
- `settings/` TOML profiles; `migrations/` database migrations; `index.html` SPA shell.
- `Makefile.toml` supported development and verification tasks.

## Applications

Each application is created with `startapp --with-pages` and owns its models,
server functions, services, routes, and client components. Other applications
see only explicit serializable contracts.

| App | Owns |
|-----|------|
| `accounts` | Users, GitHub sign-in, sessions, sign-up policy, Staff grants, Login Links, CLI Sessions (OAuth authorization server for the CLI) |
| `organizations` | Organizations, Members, roles, Invitations, tenant namespace rule |
| `clusters` | Cluster registration and the Cluster's OAuth client |
| `agents` | Agent Gateway, command outbox, Agent connection state |
| `projects` | Projects and their sources |
| `deployments` | Deployments and CLI deployment submission |
| `logs` | Log reads and realtime log delivery |
| `github` | GitHub App installations, repositories, imports, webhooks |
| `health` | Health endpoint |

Use this extraction test for every feature: "Could this feature be extracted
and moved to another project?" If not, reduce project-level coupling until it
can live inside one application. Staff-only reinhardt admin registrations are
kept for every model.

## Runtime Contract

The image built from this application is consumed by the operator and the CLI
Dockerfile generator. Keep these stable:

- Layout: `settings/`, `migrations/`, `index.html`, binaries
  `reinhardt-cloud-dashboard` and `manage`, HTTP on port 8000.
- `REINHARDT_CLOUD_CONFIG_DIR` points at the settings directory in a deployed
  image (`/app/settings`).
- `/app/manage migrate` runs as the migration init container; `manage run_worker`
  is the worker command.
- Operator-injected environment: `REINHARDT_ENV`, `REINHARDT_CORE__SECRET_KEY`,
  `REINHARDT_CLOUD_SECRET_KEY`, `REINHARDT_CLOUD_JWT_SECRET`,
  `REINHARDT_CLOUD_REDIS_URL`, `REINHARDT_CLOUD_REDIS_PASSWORD`, and
  `REINHARDT_DATABASE_{HOST,PORT,NAME,USER,PASSWORD}`.
- Sign-in additionally reads `REINHARDT_CLOUD_GITHUB_CLIENT_ID`,
  `REINHARDT_CLOUD_GITHUB_CLIENT_SECRET`, and `REINHARDT_CLOUD_PUBLIC_URL`
  (see `README.md`). `staging` and `production` refuse to start without the
  GitHub App and the public origin unless `REINHARDT_CLOUD_GITHUB_SIGN_IN=disabled`
  opts out explicitly, in which case only Login Links can sign in.

## Required Guidance

Read the relevant instruction before changing the matching surface:

- @instructions/MODULE_SYSTEM.md
- @instructions/ANTI_PATTERNS.md
- @instructions/MACRO_USAGE.md
- @instructions/PAGE_MACRO.md
- @instructions/REACTIVE_HOOKS.md
- @instructions/ORM_GUIDANCE.md
- @instructions/TESTING_STANDARDS.md
- @instructions/DOCUMENTATION_STANDARDS.md
- @instructions/REINHARDT_PAGES.md

## Conventions

### Structure

- Import framework APIs through the `reinhardt` facade with explicit imports
  (`use reinhardt::pages::component::Page;`). Do not use glob imports or the
  prelude in application code, and import types at the top of the module instead
  of repeating long paths.
- Add applications with `reinhardt-admin startapp <name> --with-pages` from this
  directory so the registries in `src/apps.rs` and `src/config/apps.rs` stay in
  sync, then merge the new application's `url_patterns()` in `src/config/urls.rs`.
- `src/config/urls.rs` only composes application routers and framework-level
  routes. It never defines endpoint handlers.
- Define HTTP handlers in an application's `server/views.rs` and `#[server_fn]`
  functions in its `server_fn/` modules; register routes in its `urls/` modules.
- Use `module.rs` plus a sibling `module/` directory. Never create `mod.rs`.
- `reinhardt-admin startapp` emits `use reinhardt::prelude::*;` in each
  application's `urls.rs`, plus a placeholder route-backed component and a
  placeholder `#[server_fn]`. After generating an application, replace the
  prelude glob with explicit imports (for example `use reinhardt::UnifiedRouter;`)
  and delete the placeholders; keep the emptied directories with `.gitkeep`.
- Put route-backed `#[component]` wrappers under `src/apps/<app>/client/components/`.
- Keep simple `Model::objects()` CRUD visible inside the endpoint. Extract a
  service only when it has a narrower contract, a reusable consumer, or an
  independently testable invariant.

### No DI types in binaries

`src/main.rs` and `src/bin/manage.rs` must stay thin launchers. Do not define
or register DI types (`#[injectable]` providers, keys, services), models,
routers, or settings fragments in a binary crate: registrations made there are
invisible to the other binary and to tests. Put them in the library crate
(`src/apps/<app>/...` or `src/config/...`) and call into the library from the
binary.

The one exception is `ProjectProvider` in `src/bin/manage.rs`: the
`CapabilityProvider` adapter that hands the library's settings to the command
framework. It is not injectable, registers nothing, and holds no state, so it
is the only non-DI type allowed in a binary.

### Management commands

- A project command lives in its application's `server/commands/` and is
  registered from `config/commands.rs`; `src/bin/manage.rs` only calls that
  registry. Implement it as a `CapabilityCommand` (clap parses and documents the
  arguments) and have it load the validated settings with `CommandRuntime::start`
  (`apps/accounts/server/commands/runtime.rs`): the command driver creates the
  ORM pool only for built-in commands, and the settings must pass the same
  validation as the server.
- The work belongs in a service under `services/server/`, which emits the audit
  event; the command only parses, prints, and maps errors.
- Standard output carries only what a script is meant to capture (the Login Link
  URL); everything else, audit records included, goes to standard error.
- Staff grants, Login Link issuance, and moving a User to another GitHub account
  are `manage` commands and nothing else (SR-18, SR-20, SR-107): never add a
  route, server function, or admin action that does any of them.
- Activation is a `manage` command, never the admin site (it needs an active
  Staff User, who may be the one that is deactivated, and the User admin is
  read-only): `deactivate-user` deactivates and ends sessions best effort,
  `reactivate-user` ends the sessions first and only then reactivates, and
  `end-sessions` ends a User's sessions. A fail-closed path that deactivates a
  User must name these commands in its error message. The User admin is
  read-only (including `is_active`), so Staff cannot reactivate a User there
  without their sessions being ended.

### Pages and UI

- UI text goes through the i18n catalog: use `I18nContext` with `t!` (or `tr`,
  `tn`, `tp`, `tnp`). Never hardcode user-visible strings. English ships first.
- Use route reverse helpers (each application's generated `reverse` helper and
  the resolved URL accessors) for `href`, `action`, and `formaction`; do not hardcode
  paths when a named route exists.
- Build static forms with `form!` and dynamic form state with `use_form`; use
  `bind:` for signal-owned controls.
- Use `watch {}` for reactive conditionals, `use_resource` for reads, and
  `use_action` for mutations. Use `use_callback` for event handlers.
- Internal redirects go through the router navigation API, not `window.location`.
- Keep shared code cfg-clean across native and `wasm32-unknown-unknown`. Use the
  generated `client` / `server` cfg aliases.

### Components

- Reach for `reinhardt-pages` primitives before writing a component:
  `ui::ActionButton` / `FormActionButton`, `ui::ActionResultPanel` /
  `ResourcePanel`, `Portal` / `mount_portal`, `form!` with `client_form:`,
  `tables::Table`, `#[layout]` with `Outlet`. Apply the design to them with
  typed `#[style_def]` styles (class values go in through `.attr("class", ..)` or
  the `styling:` entries). Write a custom component only where no primitive
  exists, and say in its module comment which primitives were checked.
- Components shared by several applications live in `src/components/`.
  Route-backed pages (routing targets, `#[component]` and `#[layout]`
  functions) always live in `src/apps/<app>/client/components/`.

### Styling

- Component styles are `#[style_def]` / `style!` definitions next to the
  component (shared ones in `src/components/`). They read tokens with `globals { ... }`
  and never declare literal colors, spacing, or radii that `tokens.css` owns.
- `static/css/tokens.css` is the only place visual values are declared. Put
  what the `style!` DSL cannot express (document and element rules, keyframes,
  font stacks, elevations, `appearance`) in `static/css/base.css` or as an
  `rc-` class in `static/css/utilities.css`. One class has one owner: never
  repeat a scoped class's declarations in static CSS.
- Never hand-edit or commit generated `__reinhardt__/components.css`, `dist/`,
  or `dist-wasm/`. No CDN, web fonts, or inline `<script>` for styling.
- Every `t!` literal needs an entry in `src/i18n/en.rs`.

### Settings and secrets

- Settings are composed from `settings/base.toml`, the `REINHARDT_ENV` profile
  (`local`, `ci`, `staging`, `production`), and `REINHARDT_*` environment
  overrides.
- Tracked profiles (`base.toml`, `ci.toml`, `staging.toml`, `production.toml`)
  hold no literal secrets; they read them with `${VAR:?message}`. `local.toml`
  is ignored by git; start from `settings/local.example.toml`.
- Export `REINHARDT_CORE__SECRET_KEY` and `REINHARDT_DATABASE_PASSWORD` for any
  command that loads settings. The server entry and the runtime `manage`
  commands validate them: an empty value or an unexpanded `${...}` / `$(...)`
  placeholder fails startup, and the `staging` and `production` profiles must
  stay hardened (see `src/config/settings.rs`). Load settings through
  `get_resolved_settings` or `get_settings`, never by building them directly.
- Emit audit events only through `crate::audit::AuditEvent` (tracing target
  `audit`); never put tokens, codes, cookies, secrets, or email addresses in
  an audit field.
- Never commit credentials, private hostnames, or personal environment values.
  Never hardcode configuration that belongs in settings.

### Anti-patterns

- No `mod.rs`, no `pub use module::*`, no obsolete wrapper modules.
- No SQL strings; use `reinhardt-query` / the ORM.
- No native-only dependencies in WASM modules; no broad call-site `#[cfg]` workarounds.
- No handlers or routes flattened into `src/config/`.
- No demo fixture IDs, sample constants, or canned text in production route actions.
- No `#[allow(...)]` without an explanatory comment; no manual cleanup that an
  early return, `?`, or panic can skip (use RAII guards).
- No tests that only compile or always pass; tests assert behavior, use
  `rstest` with Arrange-Act-Assert, use Reinhardt components, and clean up.
- Do not hand-edit `dist/` or `dist-wasm/`.
- All code comments and rustdoc are written in English.

## Verification

Run from this directory (`dashboard/`) unless noted. `manage` commands load
settings, so export the two required secrets first and point
`REINHARDT_DATABASE_HOST` / `REINHARDT_DATABASE_PORT` at a reachable PostgreSQL.

```bash
cargo run --bin manage -- check
cargo run --bin manage -- showurls
cargo make wasm-build-dev        # WASM bundle (wasm32-unknown-unknown, wasm-pack)
cargo make wasm-test             # browser tests (headless Chrome; ChromeDriver must match Chrome)
cargo nextest run --all-features
```

From the repository root, before every push:

```bash
cargo make fmt-fix && cargo make clippy-fix
cargo make fmt-check && cargo make clippy-check
cargo check --workspace --all --all-features
```

The formatting tasks need `reinhardt-admin` and `reinhardt-formatter` at the
version pinned in the root `Cargo.toml`. The production server binary reads the
Cargo manifest from the working directory, so start it from `dashboard/`
(`cargo run --bin reinhardt-cloud-dashboard`), not from the workspace root.

## Documentation

- Update `README.md` and the relevant `instructions/` file in the same change as
  commands, layout, settings, or runtime behavior.
- `AGENTS.md` and `CLAUDE.md` are a deliberate mirror pair. Keep their content
  identical except for the top-level filename.
