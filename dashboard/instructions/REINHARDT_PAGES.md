# Reinhardt Pages Guidance

## Project shape

Keep the generated entry points stable unless the routing model intentionally
changes:

- `src/bin/manage.rs` is the native management CLI.
- `src/lib.rs` contains shared exports and macro support.
- `src/client/lib.rs` launches the WASM application.
- `src/config/urls.rs` aggregates project routes.
- `src/config/apps.rs` owns the installed-app registry.
- `src/config/settings.rs` composes typed settings.

## Adding apps

Use the Pages scaffold so every registration point is updated together:

```bash
cargo run --bin manage startapp notes --with-pages
```

Verify that the command updates `src/apps.rs`, `src/config/apps.rs`,
`src/config/urls.rs`, and the app's `urls.rs`, `server_fn.rs`, and client
modules. App-level routes belong to the app; project configuration should only
mount the app aggregate.

## Feature boundaries

Use this extraction test before implementing a feature: "Could this feature be
extracted and moved to another project?" If the answer is no, reduce coupling
until the feature can live inside an app created with `startapp`.

- Keep feature-owned models, server functions, services, routes, and client
  components inside that app.
- Keep `src/config/` limited to project-wide composition, settings, and route
  mounting.
- Connect apps through explicit serializable DTOs and framework contracts; do
  not reach into another app's private modules or state.
- A feature that needs no Pages UI should remain a server-side app surface
  instead of pulling unrelated code into `src/client/`.

## Route aggregation

The project router should call app-level route functions rather than importing
individual server functions:

```rust,ignore
#[routes]
pub fn routes() -> UnifiedRouter {
    UnifiedRouter::new()
        .merge(crate::apps::notes::urls::url_patterns())
        .merge(crate::apps::accounts::urls::url_patterns())
}
```

Each app contributes one target-neutral `url_patterns()` aggregate. The
project-level `routes` function therefore needs one `merge` per app, but no
`#[cfg(server)]` or `#[cfg(client)]` branches. Keep target gates in each app's
`urls.rs` only where they protect split client/server implementation modules.

## WASM launcher

Keep the launcher inventory-driven unless the project deliberately adopts a
different client router:

```rust,ignore
ClientLauncher::new("#root")
    .register_routes_from_inventory()
    .launch()
```

## Target boundaries

- Use the generated `client`/`wasm` aliases for browser code and
  `server`/`native` aliases for native code.
- Keep serializable DTOs and target-neutral declarations outside target-only
  modules.
- Keep Tokio, database migrations, filesystem access, and management commands
  out of WASM builds.
- Event handlers inside `page!` normally do not need duplicate native branches.
- Verify both native and browser targets after changing a shared boundary.

## Components

- Use `reinhardt-pages` primitives first and style them: `ui::ActionButton` and
  `FormActionButton` (`.attr("class", ..)`), `ui::ActionResultPanel` and
  `ResourcePanel` (the slots hold the styled alert and empty state), `Portal` /
  `mount_portal` for dialogs, `form!` with `client_form:` (its `styling:` and
  `customize:` entries evaluate class expressions), `tables::Table` (browser-only
  `dom::Element` rendering), and `#[layout]` with `Outlet`.
- Write a custom component only where no primitive exists, and name the
  primitives that were checked in its module comment.
- Shared components live in `src/components/`; route-backed pages (routing
  targets) live in `src/apps/<app>/client/components/`.

## Styles and assets

- Component styles use `#[style_def] static NAME: TypeName = style! { ... };`.
  The generated stylesheet is `__reinhardt__/components.css`; `index.html`
  links it once through `static_url`. Selectors are anchored on local classes,
  tokens are referenced through `globals { token: Color; }` (typed `Color`,
  `Length`, `Number`, and similar), and unsupported properties (`animation`,
  `clip-path`, `appearance`, `vertical-align`, `border-collapse`, ...) are
  rejected at compile time.
- `:root`, themes, resets, `@keyframes`, and non-scalar tokens stay in static
  CSS under `static/css/`, registered for `collectstatic` in
  `src/config/wasm.rs`.
- Resolve images with `resolve_static("img/...")`, not hardcoded `/static/`
  paths.
- Browser APIs used by shared components live behind `src/components/browser.rs`, which
  has inert server stubs so components render the same natively.
- Browser tests live in `tests/wasm/` and are declared as explicit `[[test]]`
  targets; native test targets declare `required-features = ["with-reinhardt"]`
  so `wasm-pack test --no-default-features` skips them. Async `rstest` tests
  need `#[test_attr(wasm_bindgen_test)]`.

## Settings and artifacts

Settings load from `settings/base.toml`, the selected profile, and
`REINHARDT_` environment overrides. Update the relevant `settings/*.example.toml`
files when the settings shape changes, and never commit secrets or personal
local values. Treat `dist/` and `dist-wasm/` as generated artifacts; do not
hand-edit their contents.

## Verification

Use the Pages formatter for DSL changes, then run native and browser checks:

```bash
cargo make fmt-check
cargo make quality
cargo make wasm-build-dev
cargo make wasm-test
cargo run --bin manage check
cargo run --bin manage showurls
```
