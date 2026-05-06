//! Cross-target SPA URL resolution shim.
//!
//! Workaround for kent8192/reinhardt-web#4185 (tracked in
//! kent8192/reinhardt-cloud#541). The macro emission ungate cascade
//! (#4161 / #4166 / #4167) plus the `#[routes]` body wasm-gating
//! (#4175 / #4179, merged as `cef9afb2`) made
//! `__url_resolver_support::ResolvedUrls` reachable from wasm in
//! principle. The remaining blocker is `#[url_patterns(mode = unified)]`:
//! it substitutes `UnifiedRouter::server` for a wasm variant taking
//! `FnOnce(ServerRouterStub) -> ServerRouterStub`, but `ServerRouterStub`
//! has no methods, so any user closure containing
//! `s.endpoint(views::*)` (the canonical pattern across this project's
//! per-app `urls.rs`) fails to compile on wasm. Until upstream stubs
//! the missing methods on `ServerRouterStub` (or fully wasm-gates the
//! `.server(|s| ...)` call site in the macro output), per-app `urls`
//! modules cannot be lifted to cross-target, which means
//! `crate::config::urls` cannot be lifted, which keeps
//! `__url_resolver_support::ResolvedUrls` unreachable from wasm SPA
//! call sites.
//!
//! This shim resolves named SPA routes against the runtime
//! `ClientUrlReverser` on both targets so the same call works from
//! `client/layout.rs`, `apps/auth/client/pages/{login,register}.rs`,
//! and `client/pages/not_found.rs`.
//!
//! Ideal implementation (without workaround):
//!   - Delete this module and the `pub mod url;` declaration in
//!     `client.rs`.
//!   - Lift `#[cfg(native)]` from `dashboard/src/config.rs::pub mod urls;`.
//!   - Lift `#[cfg(native)]` from `dashboard/src/apps.rs::pub mod
//!     {clusters,health,organizations};` and `auth::urls`, gating each
//!     module's server-only submodules individually.
//!   - In each call site, replace `url_for_spa("auth:login_page")` with
//!     `crate::config::urls::__url_resolver_support::ResolvedUrls::from_global().client().auth().login_page()`.
//!   - Requires upstream kent8192/reinhardt-web#4185 to land
//!     (#4175 / #4179 are necessary but not sufficient).
//!
//! On native the resolution path is identical to the typed accessor —
//! both eventually call `ClientUrlReverser::reverse(name, params)`. The
//! difference is purely compile-time type safety, which the typed
//! accessor adds back once the upstream gap is closed.

#[cfg(target_arch = "wasm32")]
use reinhardt::pages::with_router;

#[cfg(not(target_arch = "wasm32"))]
use reinhardt::get_client_reverser;

/// Resolve an SPA route name to a path string.
///
/// `name` follows the `<app>:<route>` convention (e.g. `"auth:login_page"`).
/// Panics if the route is not registered, mirroring the typed accessor's
/// `Result::expect` semantics in the framework.
pub fn url_for_spa(name: &str) -> String {
	url_for_spa_with(name, &[])
}

/// Resolve an SPA route name with positional params.
pub fn url_for_spa_with(name: &str, params: &[(&str, &str)]) -> String {
	#[cfg(target_arch = "wasm32")]
	{
		with_router(|router| {
			router.reverse(name, params).unwrap_or_else(|err| {
				panic!("SPA route '{name}' not registered in init_router: {err}")
			})
		})
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		let reverser = get_client_reverser().unwrap_or_else(|| {
			panic!(
				"global ClientUrlReverser not registered; ensure DashboardRouter \
				 has been resolved through DI (which calls register_client_reverser \
				 inside make_router) before invoking url_for_spa"
			)
		});
		reverser.reverse(name, params).unwrap_or_else(|| {
			panic!(
				"SPA route '{name}' not registered in make_router .client(...); \
				 add it there to enable server-side reverse URL resolution"
			)
		})
	}
}
