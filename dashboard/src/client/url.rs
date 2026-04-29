//! Client-side URL resolution for the SPA.
//!
//! Provides a [`ClientUrlResolver`] implementation backed by the global
//! `pages::Router` installed by `ClientLauncher::launch()`. SPA components
//! call [`urls()`] to obtain a resolver and look up named routes registered
//! in `super::router::init_router`, eliminating hard-coded paths in `href`
//! attributes.
//!
//! Route name conventions follow the server-side `<app>:<name>` pattern.
//! See `super::router` for the registered names.
//!
//! On non-WASM targets, `with_router` is unavailable; the resolver returns
//! a stub string so the surrounding layout/page modules (which are compiled
//! cross-target but only executed in WASM) keep compiling.

use reinhardt::ClientUrlResolver;

#[cfg(wasm)]
use reinhardt::pages::with_router;

/// SPA URL resolver backed by the globally installed `pages::Router`.
///
/// Implements [`ClientUrlResolver`] by delegating to
/// `pages::with_router(|r| r.reverse(name, params))`. A misspelled or
/// unregistered route name panics fail-fast so missing wiring is caught
/// during dev/test rather than producing broken links silently.
#[derive(Debug, Default, Clone, Copy)]
pub struct DashboardUrlResolver;

impl ClientUrlResolver for DashboardUrlResolver {
	#[cfg(wasm)]
	fn resolve_client_url(&self, name: &str, params: &[(&str, &str)]) -> String {
		with_router(|router| {
			router.reverse(name, params).unwrap_or_else(|err| {
				panic!("SPA route '{name}' not registered in init_router: {err}")
			})
		})
	}

	// Native build: layout/page modules are compiled cross-target but only
	// executed inside the WASM bundle. This stub keeps the native build
	// green; it is never reached at runtime.
	#[cfg(not(wasm))]
	fn resolve_client_url(&self, _name: &str, _params: &[(&str, &str)]) -> String {
		String::new()
	}
}

/// Return the SPA URL resolver.
///
/// # Examples
///
/// ```rust,ignore
/// use reinhardt::ClientUrlResolver;
///
/// let url = urls().resolve_client_url("auth:login_page", &[]);
/// assert_eq!(url, "/login");
/// ```
pub fn urls() -> DashboardUrlResolver {
	DashboardUrlResolver
}

/// Convenience wrapper that resolves a parameterless SPA route.
///
/// Equivalent to `urls().resolve_client_url(name, &[])`. Use this in
/// `page!` `href` attributes where no path parameters apply.
pub fn url_for(name: &str) -> String {
	urls().resolve_client_url(name, &[])
}
