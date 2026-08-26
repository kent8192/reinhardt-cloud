# Reinhardt Cloud

A Reinhardt project.

## Quick links

- Full usage guide → [`../docs/tools/dashboard.md`](../docs/tools/dashboard.md)
- Deployment guide for Platform Operators → [`../docs/tools/dashboard.md#deployment-of-the-dashboard-itself-for-platform-operators`](../docs/tools/dashboard.md#deployment-of-the-dashboard-itself-for-platform-operators)
- Source of truth for Dashboard usage and configuration is the guide above; this README is a contributor-oriented summary.
- Deployment flow & component responsibilities → [`../docs/architecture/deployment-flow.md`](../docs/architecture/deployment-flow.md)

## Getting Started

### Using cargo-make (Recommended)

Install cargo-make:
```bash
cargo install cargo-make
```

Run the development server:
```bash
cargo make runserver
```

### Using manage command

```bash
# Run the development server
cargo run --bin manage runserver

# Run migrations
cargo run --bin manage migrate
```

### Using reinhardt-admin

Install [reinhardt-web](https://github.com/kent8192/reinhardt-web) CLI tools:
```bash
cargo install reinhardt-admin
```

```bash
# Create a new app
reinhardt-admin startapp myapp
```

## Common Tasks

### Development

```bash
cargo make dev              # Run checks + build + start server
cargo make dev-watch        # Development with auto-reload (requires bacon)
cargo make runserver-watch  # Start server with auto-reload (requires bacon)
```

Dashboard form styling is centralized in the UnoCSS runtime shortcuts inside
`index.html`. Prefer the shared `rc-form-*`, `rc-field`, `rc-label`,
`rc-input`, `rc-textarea`, `rc-checkbox`, and `btn-*` classes over page-local
utility strings so generated `form!` markup stays consistent across auth,
cluster, deployment, and GitHub pages.

### Database

```bash
cd dashboard && cargo make makemigrations   # Generate migrations from model changes
cd dashboard && cargo run --bin manage migrate   # Apply checked-in migrations
```

The v0.4.0-alpha.10 migration baseline is a breaking reset with six generated
app initial migrations (`auth`, `clusters`, `default`, `deployments`,
`github`, and `organizations`). It supports only an empty PostgreSQL database.
Existing migration histories, in-place data migration, and `fake-initial`
compatibility are not provided.

`cd dashboard && cargo make makemigrations` is the authoritative way to
regenerate migrations. Migration files are generated source and must not be
hand-edited.

Cluster creation uses a generated ModelForm that accepts only `name` and
`api_url`; the owning organization, active state, and agent token state are
set by the server.

### Client routes and data

The v0.4.0-alpha.10 client uses one reinhardt-pages `ClientRouter` tree. The
`#[layout]` Dashboard shell renders its child routes through `Outlet`:
`/login` and `/register` are public, while `/`, `/account`, `/clusters`,
`/deployments`, and `/github` are authenticated children.

Direct `page!({ ... })` bodies automatically capture cloneable local values in
alpha.10. Use an explicit closure form only when a reusable page factory is
needed.

Client reads use Query Client V2 generated server-function query descriptors
with `use_query`. The Launcher or SSR runtime owns the QueryClient; pages do
not install a separate client cache provider. Mutation success invalidates the
affected query keys so dependent views refetch.

Deployment log selection is canonically represented by
`/deployments?logs=<i64>` and is extracted at the component boundary as
`Query(logs): Query<Option<i64>>`. An omitted `logs` parameter produces no
selection, while a malformed value is rejected by the typed extractor.
Deployment IDs are `i64`; no UUID compatibility adapter is used.

### OAuth account linking

Normal GitHub OAuth sign-in remains independent of account linking. Starting a
link from `/account` creates a signed, short-lived intent bound to the
initiating valid session. The callback links an identity only when its current
`sessionid` still validates and matches that intent's user and session binding;
logout, session rotation, or a session swap invalidates the link flow.

### Testing

```bash
cargo make test             # Run all tests (native nextest + WASM browser E2E)
cargo make test-unit        # Run unit tests only
cargo make test-integration # Run integration tests only
cargo make test-watch       # Run tests with auto-reload (requires bacon)
```

### Project Management

```bash
cargo make check            # Check project for common issues
cargo make showurls         # Display all registered URL patterns
cargo make shell            # Run an interactive Rust shell (REPL)
cargo make collectstatic    # Collect static files into STATIC_ROOT
```

### Code Quality

```bash
cargo make fmt-check        # Check code formatting
cargo make fmt-fix          # Fix code formatting
cargo make clippy-check     # Check linting rules
cargo make clippy-fix       # Fix linting issues
cargo make quality          # Run all checks (format + lint)
cargo make quality-fix      # Fix all issues automatically
```

### Build

```bash
cargo make build            # Build in debug mode
cargo make build-release    # Build in release mode
cargo make ci               # Run CI pipeline (format, lint, build, test)
```

### Help

```bash
cargo make help             # Show all available tasks
```

## Generated with

This project was created using `reinhardt-admin startproject`.
