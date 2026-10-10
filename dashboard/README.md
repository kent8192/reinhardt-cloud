# Reinhardt Cloud Control Plane

The Control Plane application of Reinhardt Cloud, built as a Reinhardt Pages
project (WASM frontend with server-side rendering). The Cargo package and the
server binary are named `reinhardt-cloud-dashboard`; the library crate is
`cloud_control_plane`. Application boundaries and conventions are described in
[`CLAUDE.md`](CLAUDE.md).

## Prerequisites

- Rust 1.96.0 or later (2024 Edition)
- cargo-make: `cargo install cargo-make`
- wasm-pack for the browser bundle (installed by `cargo make install-tools`)
- PostgreSQL (required by `manage check`, `migrate`, and the server)

## Getting Started

### Install Tools

```bash
# Install all development tools (includes WASM build tools)
cargo make install-tools
```

### Settings

Commands that load settings need a unique `REINHARDT_CORE__SECRET_KEY` and the
database password in `REINHARDT_DATABASE_PASSWORD`; the host, port, name, and
user default to a local PostgreSQL and can be overridden with
`REINHARDT_DATABASE_{HOST,PORT,NAME,USER}`. Copy `settings/local.example.toml`
to `settings/local.toml` (ignored by git) for local overrides.

The server entry (`reinhardt-cloud-dashboard`) and the runtime `manage`
commands validate settings when they load them and stop with an error that
names the offending setting (never its value) when a required secret is empty
or still holds an unexpanded `${...}` or `$(...)` placeholder, when the token
encryption key or the sign-up allowlists are malformed, or when the `staging`
or `production` profile is not hardened (debug off, secure cookies, HTTPS
redirect with HSTS, explicit allowed hosts and `https` WebSocket origins
without wildcards or localhost). Static `manage` commands that resolve only
the settings they need, such as `collectstatic`, skip this validation by
design.

The `staging` and `production` profiles also require
`REINHARDT_CLOUD_REDIS_URL` (the operator injects it with the Redis password
expanded). Optional variables, all read through `settings/base.toml`:

| Variable | Effect |
|----------|--------|
| `REINHARDT_CLOUD_SIGN_UP_POLICY` | `open`, `allowlist`, or `invite_only` (default). Any other value means `invite_only`. |
| `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_USER_IDS` | Comma-separated numeric GitHub user IDs admitted under `allowlist`. |
| `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_ORGANIZATION_IDS` | Comma-separated numeric GitHub organization IDs whose members are admitted under `allowlist`. IDs, not logins: a renamed organization's old login can be claimed by someone else. Membership is read with the signing-in User's own token from `GET /user/memberships/orgs?state=active`, so the GitHub App needs the "Members" organization permission and must be installed on the organization; a pending invitation does not count. |
| `REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY` | Base64 of 32 random bytes, the key that encrypts GitHub tokens at rest. When unset, a key is derived from `REINHARDT_CORE__SECRET_KEY`. |
| `REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY_ID` | Identifier stored with every ciphertext; defaults to `primary`. |
| `REINHARDT_CLOUD_TOKEN_ENCRYPTION_RETIRED_KEYS` | Comma-separated `id:base64` pairs kept to decrypt tokens sealed before a key rotation. |
| `REINHARDT_CLOUD_GITHUB_CLIENT_ID`, `REINHARDT_CLOUD_GITHUB_CLIENT_SECRET` | Client ID and secret of the GitHub App behind sign-in. Set both together. `staging` and `production` refuse to start without them (and without `REINHARDT_CLOUD_PUBLIC_URL`) unless `REINHARDT_CLOUD_GITHUB_SIGN_IN=disabled` opts out; a Control Plane started that way can only be entered with a Login Link. The App's callback URL is `<public URL>/api/auth/github/callback/`. |
| `REINHARDT_CLOUD_GITHUB_SIGN_IN` | `disabled` runs the Control Plane without GitHub sign-in. Any other non-empty value stops startup. |
| `REINHARDT_CLOUD_PUBLIC_URL` | Origin the Dashboard is served from; with the GitHub App configured it must be a bare origin (scheme and host, no path) in every profile. The GitHub callback URL and the first allowed cross-site request origin derive from it. `staging.toml` and `production.toml` name the deployed origins and require `https` when the GitHub App is configured. |
| `REINHARDT_CLOUD_TRUSTED_PROXIES` | Comma-separated IP addresses (exact addresses; the framework supports no ranges) of the TLS-terminating proxies in front of the Control Plane. Only requests from these addresses have `X-Forwarded-Proto` honored, which is what lets `Strict-Transport-Security` be sent behind a proxy. Unset, HSTS is never sent (a startup warning says so in a deployed profile). |
| `REINHARDT_CLOUD_ALLOWED_ORIGINS` | Comma-separated extra origins allowed to send state-changing cookie-authenticated requests. A wildcard is ignored; loopback origins exist only in a debug profile. |
| `REINHARDT_CLOUD_GITHUB_AUTHORIZE_URL`, `REINHARDT_CLOUD_GITHUB_TOKEN_URL`, `REINHARDT_CLOUD_GITHUB_API_URL` | Endpoint overrides that point sign-in at a local stand-in for GitHub (tests only). |

### Sign-in and the request surface

Browser sign-in is GitHub only (a GitHub App's user authorization with PKCE).
The sign-in state is single-use, expires within ten minutes, and is bound to
the browser by a short-lived `HttpOnly` cookie; sessions live in Redis with a
30-minute idle and 24-hour absolute limit, are rotated at every sign-in, and are
destroyed on the server at sign-out. Privileges are read from the current
`User` row on every request. Everything under `/api/` and `/admin` rejects
anonymous callers except the short list in
`src/config/middleware/access_gate.rs`; the admin site additionally requires
Staff. State-changing cookie-authenticated requests must carry an allowed
`Origin`. The OpenAPI document, Swagger UI, and ReDoc are served only in the
`local` and `ci` profiles. The security headers cover everything the router
serves; the single-page application shell and static assets are answered by the
framework's static layer, which cannot carry them yet (reinhardt-web#6721).

### Operator access: Staff, Login Links, and moving a User

Three operations are available only as `manage` commands, so each needs host
operator access (a shell on the host or in the container, with the Control
Plane's settings in its environment). No HTTP, WebSocket, or gRPC endpoint,
Invitation, or CLI Session can do any of them, and each emits an audit event with
the numeric GitHub user ID.

```bash
# Pre-provision the first Staff User of a new deployment (SR-105), or grant Staff.
# The account need not have signed in: it is created from the ID alone and can
# sign in with GitHub whatever the sign-up policy is. The exemption is for that
# one ID. Add --revoke to remove Staff and end the User's sessions (a User who was
# pre-provisioned and never signed in is removed with it). Revoking a User who is
# not Staff changes nothing and ends no session, like a repeated grant.
manage grant-staff --github-user-id 583231
manage grant-staff --github-user-id 583231 --revoke

# Print a single-use sign-in URL for an existing, active User (break-glass access,
# automation, a deployment without GitHub sign-in). The URL is the only thing on
# standard output, once; it is stored only as a hash and cannot be shown again.
# --ttl-minutes shortens the default 10 minutes; 15 is a hard ceiling.
url=$(manage create-login-link --github-user-id 583231 --ttl-minutes 5)

# Move a User to another GitHub account. Memberships, roles, and Staff stay; the
# old account's stored tokens and unused Login Links are removed and every session
# ends. An ID that another User already has is refused. If Redis cannot be reached
# after the move committed, the User is deactivated (leftover sessions are then
# refused on their next request) and the command fails; recover as below.
manage repoint-github-account --github-user-id 583231 --new-github-user-id 9919

# Recovery, once Redis is reachable again. The admin site is not a recovery path:
# it needs an active Staff User, and the User who needs recovering may be the only
# one. It also cannot reactivate anyone: the User admin is read-only (activation,
# Staff, and identity change only through these commands), because reinhardt-admin
# has no hook that could end the sessions first. end-sessions ends every browser session of the User (non-zero exit if Redis
# fails). reactivate-user ends every session first, and only then sets the User
# active again, because a session left in Redis while the User was inactive would
# otherwise become valid again; it refuses if the sessions cannot be ended and
# changes nothing for a User who is already active. Use the User's current
# (new) GitHub user ID.
manage end-sessions --github-user-id 9919
manage reactivate-user --github-user-id 9919

# Deactivate a User (they can no longer sign in, and every session of theirs is
# refused on its next request because the flag is read from the database on every
# request). The sessions are then ended on a best-effort basis: if Redis is down
# the command still succeeds, says so, and audits `accounts.deactivate.failed`;
# run end-sessions later to remove them. An already-inactive User is unchanged.
manage deactivate-user --github-user-id 9919
```

These three commands, `deactivate-user`, `reactivate-user`, and `end-sessions`,
are the whole activation path: the User admin is read-only (including
`is_active`), because reinhardt-admin has no hook that could end a User's sessions
before an admin-side reactivation (kent8192/reinhardt-web#6725, tracked in
kent8192/reinhardt-cloud#956).

A Login Link looks like `<REINHARDT_CLOUD_PUBLIC_URL>/sign-in/link/#<secret>`.
The secret is in the URL fragment, which a browser never sends, so loading the
page puts it in no access log, proxy log, or `Referer`, and a link previewer or
mail scanner that only fetches the URL consumes nothing. The page shows a
"Sign in" button; pressing it sends the secret in a POST from the Dashboard's
own origin. Consumption is one atomic conditional update, so a link works once
even when two browsers race for it, and an unknown, used, expired, or
deactivated-User link all get the same answer. The session it creates is the
same one GitHub sign-in creates, and it counts as a sign-in: a pre-provisioned
User who signed in with a Login Link keeps their row when Staff is revoked and is
only demoted, because the row is removed only for a User who never signed in. The
URL needs `REINHARDT_CLOUD_PUBLIC_URL` to be
an origin; the command refuses otherwise.

The audit records are written as JSON lines on standard error, filtered by
`RUST_LOG` (default `info`), by the server and by every `manage` command
(`logging::init`, called by `manage` itself). Nothing else installs a log
output, so without this the audit events would be discarded.

### Audit events

Security-relevant decisions are recorded through `crate::audit::AuditEvent`: a
`tracing` event at `info` level with target `audit` and the fields `event`,
`actor_kind`, `actor_user_id`, `subject_user_id`, `github_user_id`, `outcome`,
and `reason`. The helper has no field for tokens, codes, cookies, secrets, or
email addresses, and `reason` accepts only a static code.

### Development Server

```bash
# Build WASM and start development server
cargo make dev

# Or step by step:
cargo make wasm-build-dev    # Build WASM
cargo make runserver         # Start server
```

Visit `http://127.0.0.1:8000/` in your browser.

`manage runserver` serves the HTTP and WebSocket routes from the project
router. If any application contributes gRPC services, the same command also
starts the gRPC listener on `127.0.0.1:50051`; override it with
`--grpc-address`. The generated application `urls.rs` files are merged from
`src/config/urls.rs`. `src/main.rs` is only the container entry point: it starts
the same server on `0.0.0.0:8000` from a prebuilt bundle in `/app/static/wasm`.
Browser WebSocket origins are allow-listed in `settings/base.toml`; replace the
generated local origins with the deployed HTTPS origin in production.

### Build for Production

```bash
# Build WASM (release + optimized)
cargo make wasm-build-release

# Build the server and management binaries
cargo build --release -p reinhardt-cloud-dashboard
```

## Project Structure

```
dashboard/
├── src/
│   ├── audit.rs      # Shared audit-event helper (`tracing` target `audit`)
│   ├── logging.rs    # Process-wide `tracing` output (JSON lines on stderr)
│   ├── main.rs       # Server binary (container entry point)
│   ├── server.rs     # Server bootstrap used by main.rs (ORM pool, documentation switch)
│   ├── config/       # Settings, project routes, admin site, request-surface middleware
│   ├── bin/manage.rs # Management binary
│   ├── client/       # WASM entry point (runs in browser)
│   │   └── lib.rs    # `wasm_bindgen(start)` launcher
│   ├── i18n.rs       # Message catalogs and the page i18n context
│   ├── components.rs # Components shared by every app (see "Components")
│   ├── components/   # Styled primitives, badge, code block, theme toggle, layouts
│   ├── apps/         # App modules generated by startapp
│   │   ├── accounts.rs
│   │   └── accounts/
│   │       ├── serializers.rs # Client/server wire DTO declarations
│   │       ├── server_fn.rs   # Server-function declaration surface
│   │       ├── services.rs    # Client/server service declaration surface
│   │       ├── urls.rs        # Target-gated app route exports
│   │       ├── client/        # WASM-only UI and client services
│   │       ├── serializers/   # DTO implementations
│   │       ├── server_fn/     # Server-function implementations
│   │       ├── services/      # Split client/server service implementations
│   │       ├── urls/          # Split client/server route implementations
│   │       └── server/        # Native-only models/forms/views/admin wiring
│   └── config/       # Settings, routes, admin site, middleware, `manage` command registry
├── migrations/       # Database migrations
├── settings/         # TOML profiles (base, ci, staging, production; local is ignored)
├── static/           # Design tokens, base styles, utilities, images (collected by `collectstatic`)
├── tests/wasm/       # Browser tests (`cargo make wasm-test`)
├── dist/             # WASM build output
├── dist-wasm/        # wasm-pack output copied into dist/
├── index.html        # WASM entry HTML
├── scripts/          # cargo-make helper scripts
└── Cargo.toml
```

## Styling and Design System

The visual design comes from the approved prototype in
`docs/design/control-plane-prototype/`. Styles reach the browser through two
channels, both published by `collectstatic` and linked once from `index.html`:

- **Component styles** are typed `#[style_def] static ... = style! { ... }`
  definitions in `src/components/`. The extractor compiles them into the generated
  `__reinhardt__/components.css`; never edit or commit that output.
  Components read design tokens through `globals { ... }` and compose scoped
  class tokens such as `BUTTON_STYLES.primary()`.
- **Static CSS** in `static/css/` holds what the style DSL cannot express:
  `tokens.css` (custom properties and both themes), `base.css` (document and
  element rules), and `utilities.css` (`rc-` classes for font stacks,
  elevations, easing, keyframes, `appearance`, and similar). `tokens.css` is the
  only place visual values are declared.

The theme follows the system setting until the toggle in the signed-out layout
sets `data-theme` on `<html>` and stores the choice in local storage under
`rc-theme`; `static/js/theme-init.js` re-applies it before first paint.

### Components

Use the `reinhardt-pages` primitives first (`ui::ActionButton`,
`ui::FormActionButton`, `ui::ActionResultPanel`, `ui::ResourcePanel`,
`Portal` / `mount_portal`, `form!` with `client_form:`, `tables::Table`) and apply
the design to them with the typed styles in `src/components/`. A custom
component exists only where no primitive does, and says which primitives were
checked.

- Components shared by several applications live in `src/components/`.
- Route-backed pages (routing targets: `#[component]` / `#[layout]` functions) live in
  `src/apps/<app>/client/components/` of the owning application.

User-visible text goes through `t!`; add every new message to
`src/i18n/en.rs` (a test fails when a `t!` literal has no entry).

## Management Commands

```bash
# Create a new app (run from this directory, then merge its routes in src/config/urls.rs)
reinhardt-admin startapp myapp --with-pages

# Database migrations (when using database features)
cargo run --bin manage makemigrations
cargo run --bin manage migrate

# Check project for issues
cargo run --bin manage check

# List registered server URL patterns
cargo run --bin manage showurls

# Run with a custom gRPC listener
cargo run --bin manage runserver --grpc-address 127.0.0.1:50061

# Export the deterministic application contract
cargo run --bin manage contract export --format json

# Operator access (see "Operator access" above)
cargo run --bin manage grant-staff --github-user-id <id> [--revoke]
cargo run --bin manage create-login-link --github-user-id <id> [--ttl-minutes <n>]
cargo run --bin manage repoint-github-account --github-user-id <id> --new-github-user-id <id>
cargo run --bin manage end-sessions --github-user-id <id>
cargo run --bin manage reactivate-user --github-user-id <id>
cargo run --bin manage deactivate-user --github-user-id <id>
```

The project-specific commands are registered in `src/config/commands.rs`, one
line per application that contributes commands.

### Rust management shell (opt-in)

The `commands-shell` feature is not enabled by default. Enable it explicitly to
get an interactive Rust shell with the settings, ORM handle, and DI context bound:

```bash
cargo run --bin manage --features commands-shell -- shell
cargo run --bin manage --features commands-shell -- shell -c \
  'println!("{}", settings.core.debug)'
```

`src/config/shell.rs` supplies the shell configuration. The shell is not a sandbox.

## WASM Build Commands

```bash
cargo make wasm-build-dev      # Build WASM (debug)
cargo make wasm-build-release  # Build WASM (release + optimize)
cargo make wasm-watch          # Watch and rebuild on changes
cargo make wasm-clean          # Clean WASM build artifacts
cargo make wasm-test           # Browser tests in headless Chrome
```

`wasm-test` needs a ChromeDriver that matches the installed Chrome major
version on `PATH`.

## Learn More

- [Generated `page!` Macro Guide](instructions/PAGE_MACRO.md)
- [Generated Reactive Hooks Guide](instructions/REACTIVE_HOOKS.md)
- [Generated ORM and Migration Guide](instructions/ORM_GUIDANCE.md)
- [Reinhardt Documentation](https://github.com/kent8192/reinhardt-web)
- [Reinhardt Pages Guide](https://github.com/kent8192/reinhardt-web/tree/main/docs)
- [wasm-pack Documentation](https://rustwasm.github.io/wasm-pack/)
