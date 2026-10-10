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
| `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_ORGANIZATION_IDS` | Comma-separated numeric GitHub organization IDs whose members are admitted under `allowlist`. IDs, not logins: a renamed organization's old login can be claimed by someone else. |
| `REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY` | Base64 of 32 random bytes, the key that encrypts GitHub tokens at rest. When unset, a key is derived from `REINHARDT_CORE__SECRET_KEY`. |
| `REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY_ID` | Identifier stored with every ciphertext; defaults to `primary`. |
| `REINHARDT_CLOUD_TOKEN_ENCRYPTION_RETIRED_KEYS` | Comma-separated `id:base64` pairs kept to decrypt tokens sealed before a key rotation. |

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
│   ├── main.rs       # Server binary (container entry point)
│   ├── server.rs     # Server bootstrap used by main.rs
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
│   └── config/       # Server configuration
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
```

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
