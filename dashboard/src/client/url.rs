//! Cross-target SPA URL resolution shim.
//!
//! Workaround for kent8192/reinhardt-web#4119 (tracked in
//! kent8192/reinhardt-cloud#534). The framework's typed accessor
//! `urls.client().<app>().<route>()` is gated behind `is_wasm_target()`
//! in `routes_registration.rs:472`, so per-app methods are not generated
//! on `wasm32-unknown-unknown` and any cross-target SPA file that calls
//! them fails to compile. This shim resolves named SPA routes against
//! the runtime `ClientUrlReverser` on both targets so the same call
//! works from `client/layout.rs`, `apps/auth/client/pages/{login,
//! register}.rs`, and `client/pages/not_found.rs`.
//!
//! Ideal implementation (without workaround):
//!   - Delete this module.
//!   - In each call site, replace `url_for_spa("auth:login_page")` with
//!     `crate::config::urls::ResolvedUrls::from_global().client().auth().login_page()`.
//!   - The typed accessor needs to be available on wasm too (upstream
//!     kent8192/reinhardt-web#4119).
//!
//! On native the resolution path is identical to the typed accessor —
//! both eventually call `ClientUrlReverser::reverse(name, params)`. The
//! difference is purely compile-time type safety, which the typed
//! accessor adds back once the upstream gate is lifted.

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
