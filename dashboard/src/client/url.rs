//! Cross-target SPA URL resolution shim.
//!
//! Workaround for kent8192/reinhardt-web#4175 (tracked in
//! kent8192/reinhardt-cloud#541). The four `#[cfg(native)]` re-exports
//! that originally blocked cross-target use of `#[url_patterns]` /
//! `#[app_config]` (kent8192/reinhardt-web#4161) were ungated by #4166
//! and #4167, so per-app `urls.rs` files now compile cross-target —
//! `dashboard/src/apps/dashboard.rs` and its `urls.rs` / `urls/ws_urls.rs`
//! children no longer need their `#[cfg(native)]` gates.
//!
//! The remaining gap blocking the typed accessor is upstream #4175: the
//! `#[routes]` macro's generated `__url_resolver_support::ResolvedUrls`
//! type is co-located with the `routes()` function it annotates, so the
//! function body's wasm typecheck failure transitively kills the type.
//! Our project-level `routes()` body in `crate::config::urls` references
//! native-only items (admin routes, `RedisSessionBackend`, four session /
//! security middleware re-exports, `#[inject]` parameters, server
//! functions) — making `crate::config::urls` cross-target produces ~40
//! errors. Until upstream emits `ResolvedUrls` outside the function's
//! cfg scope (or documents a wasm-friendly split-routes pattern), the
//! type stays unreachable from wasm SPA call sites.
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
//!   - In each call site, replace `url_for_spa("auth:login_page")` with
//!     `crate::config::urls::ResolvedUrls::from_global().client().auth().login_page()`.
//!   - Requires upstream kent8192/reinhardt-web#4175 to land.
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
