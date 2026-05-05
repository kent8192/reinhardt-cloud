//! Cross-target SPA URL resolution shim.
//!
//! Workaround for kent8192/reinhardt-web#4161 (tracked in
//! kent8192/reinhardt-cloud#540). The framework's typed accessor
//! `urls.client().<app>().<route>()` is reachable in principle on
//! `wasm32-unknown-unknown` after kent8192/reinhardt-web#4132 (per-app
//! emission) and #4156 (`routes`/`url_patterns` macro re-export ungate),
//! but the macro expansions still reference four downstream-facing
//! re-exports/modules that remain `#[cfg(native)]` in
//! `reinhardt-web/src/lib.rs`:
//!
//!   - `reinhardt_apps`     (`:118`, `cfg(all(feature = "core", native))`)
//!   - `urls`               (`:243`, `cfg(native)`)
//!   - `app_config`         (`:255`, `cfg(native)`)
//!   - `WebSocketRouter` &c (`:1201`, `cfg(native)`)
//!
//! As a result, `dashboard/src/apps/dashboard.rs` and
//! `apps/dashboard/urls.rs` must remain `#[cfg(native)]`, which leaves
//! the `ResolvedUrls` struct unreachable from cross-target SPA call
//! sites. This shim resolves named SPA routes against the runtime
//! `ClientUrlReverser` on both targets so the same call works from
//! `client/layout.rs`, `apps/auth/client/pages/{login,register}.rs`,
//! and `client/pages/not_found.rs`.
//!
//! Ideal implementation (without workaround):
//!   - Delete this module and the `pub mod url;` declaration in
//!     `client.rs`.
//!   - Ungate `dashboard/src/apps/dashboard.rs` and
//!     `apps/dashboard/urls.rs` (drop `#[cfg(native)]`).
//!   - In each call site, replace `url_for_spa("auth:login_page")` with
//!     `crate::config::urls::ResolvedUrls::from_global().client().auth().login_page()`.
//!   - Requires the four re-exports above to ungate cross-target
//!     (upstream kent8192/reinhardt-web#4161).
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
