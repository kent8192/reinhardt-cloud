# Reinhardt alpha.11 to alpha.14 migration coverage

The Dashboard uses `reinhardt-web = "=0.4.0-alpha.14"`, matching
`reinhardt-pages` and test dependencies, and pinned alpha.14 migration/format
tools. The published `reinhardt-event-catalog 0.4.0-alpha.1` is an intentional
transitive dependency of this release.

The [upstream release range](https://github.com/kent8192/reinhardt-web/compare/reinhardt-web%40v0.4.0-alpha.11...reinhardt-web%40v0.4.0-alpha.14)
contains 524 commits, from baseline `8e5f998ebca51877c5cc5a868c983c9b38e63e1d`
to release head `77fa2bc4d2af892b3427b61e56180f6d84a88cca`. Commit-to-PR
associations identify 27 merged PRs in that ancestry, plus release PR #6265
whose head is the alpha.14 tag. All 28 PR bodies and complete changed-file
lists were inspected against the released source.

Release tags point to release PR heads before their merge commits. Therefore
#6194 appears in the range even though its alpha.11 metadata is already in the
baseline; #6211, #6252, and #6265 publish alpha.12, alpha.13, and alpha.14.

## PR coverage

Paths below are relative to the repository root. “Inherited” means the pinned
framework/tool dependency supplies the change without a Cloud implementation.

| Upstream PR | Released change | Dashboard application |
| --- | --- | --- |
| [#6194](https://github.com/kent8192/reinhardt-web/pull/6194) | Alpha.11 release metadata | Baseline metadata; superseded by the alpha.14 manifest, lockfile, tool, and image pins. |
| [#6198](https://github.com/kent8192/reinhardt-web/pull/6198) | Alpha.11 announcement | Documentation only; no consumer code. |
| [#6200](https://github.com/kent8192/reinhardt-web/pull/6200) | `#[client_form]` attribute | Auth serializers and cluster/deployment/GitHub request DTOs use the attribute instead of the removed derive. |
| [#6201](https://github.com/kent8192/reinhardt-web/pull/6201) | Opt-in DTO schemas | Auth serializers explicitly use `#[dto(schema)]`; plain DTOs do not acquire unused schemas. |
| [#6202](https://github.com/kent8192/reinhardt-web/pull/6202) | Async session-backed, browser-bound OAuth state | `dashboard/src/apps/auth/services/oauth/backend.rs` uses Redis `AsyncSessionStateStore`; `dashboard/src/apps/auth/server_urls/oauth.rs` uses contextual start/callback with a nonce cookie and server-only link identity. |
| [#6203](https://github.com/kent8192/reinhardt-web/pull/6203) | `miniz_oxide` duplicate allowance | Upstream cargo-deny policy only. Cloud uses its existing cargo-audit policy and has no matching deny configuration. |
| [#6206](https://github.com/kent8192/reinhardt-web/pull/6206) | Main-to-develop synchronization | Reviewed overlapping OAuth/schema changes and feature wiring; enabled facade `social-auth` for the session state adapter. Remaining framework changes are inherited. |
| [#6207](https://github.com/kent8192/reinhardt-web/pull/6207) | `pem`/`miniz_oxide` duplicate allowances | Upstream cargo-deny policy only; no Cloud policy copy is needed. |
| [#6211](https://github.com/kent8192/reinhardt-web/pull/6211) | Alpha.12 release metadata | Superseded by the final alpha.14 pins. |
| [#6213](https://github.com/kent8192/reinhardt-web/pull/6213) | Alpha.12 announcement | Documentation only; no consumer code. |
| [#6226](https://github.com/kent8192/reinhardt-web/pull/6226) | Semantic native input bindings | Auth and operation controls use `bind:` with their semantic HTML types, including password/email/URL; no input-type substitution or manual auth input adapter remains. |
| [#6227](https://github.com/kent8192/reinhardt-web/pull/6227) | Versioned migration source and offline upgrader | All seven checked-in migrations use generated versioned builder source. `.github/workflows/fmt.yml` checks source compatibility with the pinned CLI. Schema/history are preserved. |
| [#6231](https://github.com/kent8192/reinhardt-web/pull/6231) | Date/time bindings | Inherited. Existing Dashboard forms contain no date/time/month/week controls requiring conversion. |
| [#6232](https://github.com/kent8192/reinhardt-web/pull/6232) | Native filesystem import gating | Inherited native/WASM dependency fix; Cloud has no copy of the affected upstream module. |
| [#6235](https://github.com/kent8192/reinhardt-web/pull/6235) | Target-neutral server mutations | Auth and operation forms use generated mutation adapters; layout logout uses `use_server_mutation`. Pending/error handling and duplicate-submit protection come from the runtime. |
| [#6237](https://github.com/kent8192/reinhardt-web/pull/6237) | Non-generic target-neutral `UnifiedRouter` | Project/app URL declarations use the new type and omit inert identity builders. The WASM launcher owns one live tree; native client-route tests use `ReactiveScope`. |
| [#6238](https://github.com/kent8192/reinhardt-web/pull/6238) | Structured DB constraints and model-aware errors | Registration/cluster conflicts use model constraint metadata; Personal Organization retries use `DatabaseErrorKind`. Native `reinhardt-pages/model-server-fnset` enables the public mapper without adding DB code to WASM. |
| [#6239](https://github.com/kent8192/reinhardt-web/pull/6239) | Native filesystem test helper | Upstream test-only repair; no consumer implementation. |
| [#6240](https://github.com/kent8192/reinhardt-web/pull/6240) | Typed field binding and synchronized reset | ClientForm controls use generated runtime field enums. Custom cluster ModelForm controls use public setters/bound signals and reset callbacks, replacing DOM ID resets. |
| [#6244](https://github.com/kent8192/reinhardt-web/pull/6244) | Async layout/route guards | The shared dashboard layout checks `me` before protected children mount: 401 redirects to login, 403 forbids navigation. Login and logout invalidate framework authentication, including cached queries. Both use the target-neutral replacement-navigation API. |
| [#6248](https://github.com/kent8192/reinhardt-web/pull/6248) | Named target-neutral ModelForm contracts | The real `Cluster` model declares `form(name = ClusterCreateForm, fields(name, api_url))`; its generated contract replaces the WASM shadow model. Native-only model relations remain excluded from the browser build. |
| [#6249](https://github.com/kent8192/reinhardt-web/pull/6249) | Authoritative generated normalization/validation | Cluster creation calls generated `clean_and_validate` before persistence; model form metadata trims name/URL and applies declared constraints. Server request validation remains authoritative for ClientForm DTOs. |
| [#6250](https://github.com/kent8192/reinhardt-web/pull/6250) | Admin object-scope/alias regression repair and page security feature wiring | Inherited by existing `UserAdmin`, `ClusterAdmin`, and `DeploymentAdmin` registrations; no alternate admin view or feature workaround is needed. |
| [#6251](https://github.com/kent8192/reinhardt-web/pull/6251) | Typed-form format/lint repair | Inherited; local forms are checked with the matching formatter and Clippy. |
| [#6252](https://github.com/kent8192/reinhardt-web/pull/6252) | Alpha.13 release metadata | Superseded by the final alpha.14 pins. |
| [#6253](https://github.com/kent8192/reinhardt-web/pull/6253) | Alpha.13 announcement | Documentation only; no consumer code. |
| [#6264](https://github.com/kent8192/reinhardt-web/pull/6264) | Restore upload metadata in standalone macro fixtures | Upstream test-only repair; the Dashboard has no file-upload form to migrate. |
| [#6265](https://github.com/kent8192/reinhardt-web/pull/6265) | Alpha.14 release metadata | `Cargo.toml`, `Cargo.lock`, Dashboard dependencies, CLI/formatter install pins, and container tooling match the tagged release. |

## Application boundaries

- OAuth state/PKCE are shared through Redis and consumed once. The browser
  cookie contains an opaque binding nonce; link ownership is server-side and
  requires the same active session at callback time.
- The browser navigation guard reads the generated `me()` endpoint at the
  framework's prepare and pre-commit checkpoints. Native rendering has no
  browser cookie context and fails closed. The mounted shell retains its
  existing 60-second session revalidation. Its `hidden` attribute uses
  `PageElement::reactive_attr` so session changes reveal/hide the same form DOM.
- Session transitions use `invalidate_authentication` and `navigate_or_reload`
  with replacement navigation, so cached identities are cleared without
  maintaining an application-specific query-family list or browser-only helpers.
- Alpha.14 named ModelForms do not expose ClientForm's typed field bindings.
  Cluster custom controls therefore use the released string field setters
  and signals; the model remains the sole source of form/schema metadata.
- Cluster updates trim the name and API URL before DTO validation, matching
  creation's normalization order and character-based length constraints.
- Overview counts use the organization-scoped cluster and deployment queries.
  Loading or failed queries never imply an empty organization. Health badges
  require an authoritative health source and are omitted from the overview.
- The log viewer is compiled only in the WASM component module. Its shared
  style helpers remain covered by native tests; native routes omit the viewer.
- Browser E2E sessions own a dedicated transport runtime and delete their
  WebDriver session in `Drop`, including assertion failures and cancellation.
- The standalone gRPC startup remains necessary: Cloud's tonic 0.13 services,
  JWT/token validators, health/reflection registration, and shutdown hook are
  not registered with `UnifiedRouter`. Removing `config/grpc.rs` requires
  migrating that complete service configuration and its tonic/prost versions.
- Source-version conversion does not change migration identities or schema.
  The alpha.11 empty-PostgreSQL baseline requirement remains in force.

## Regression checks

The native suite covers Redis state consumption across independent OAuth
backends, browser/session binding, structured constraint mapping, generated
cluster normalization, public/protected route metadata, and native guard
rejection without protected HTML. WASM browser tests cover guard denial before
cluster fetching, accepted navigation/history, auth input identity/focus, and
cluster form submission/reset through the generated server-function transport.
Login navigation also exercises organization-scoped overview counts, while
native tests cover authentication cache invalidation and WebDriver session
deletion on normal return, assertion failure, and test-future cancellation.

Run the repository's `cargo make test`, `cargo make fmt-check`,
`cargo make clippy-check`, and workspace check/build/doc commands with the
pinned tools. The migration source check is
`reinhardt-admin migrations upgrade-source dashboard/migrations --check`.

Validation on 2026-09-07 used Rust 1.96.0, alpha.14 CLI/formatter tools,
Docker-backed infrastructure, and Chrome 152 with a matching ChromeDriver.

| Check | Result |
| --- | --- |
| Workspace check/build with all features and the locked dependency graph | Passed. |
| `cargo make test-native` | 2,465 passed; 4 skipped. |
| Dashboard `cargo make wasm-spa-test` | 3 browser tests passed. |
| `cargo make fmt-check` | Passed; no files require formatting. |
| `cargo make clippy-check` | Passed with warnings denied. |
| Migration source compatibility check | All seven migrations passed without rewriting files. |
| `cargo make doc-test` | 1 passed; 14 ignored. |
| Workspace documentation build | Completed with existing link and output-name collision warnings. |
| `cargo make audit` | Passed under the existing policy, with four allowed warnings: `proc-macro-error2`, `event-listener`, and the yanked `chacha20`/`spin` versions. |
