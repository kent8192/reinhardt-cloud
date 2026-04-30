//! Client-side URL resolution for the SPA.
//!
//! Provides a [`ClientUrlResolver`] implementation that resolves named
//! SPA routes (e.g. `auth:login_page`) to URL paths. SPA components call
//! [`urls()`] to look up routes registered in
//! `super::router::init_router`, eliminating hard-coded paths in `href`
//! attributes.
//!
//! Route name conventions follow the server-side `<app>:<name>` pattern.
//! See `super::route_table::SPA_ROUTES` for the registered names.
//!
//! # Cross-target behaviour
//!
//! - **WASM**: delegates to the globally installed `pages::Router`
//!   (built by [`super::router::init_router`]) via
//!   `pages::with_router(|r| r.reverse(name, params))`.
//! - **Native**: looks up the path in [`super::route_table::SPA_ROUTES`].
//!   Required for SSR `href` generation and any server-side use of
//!   [`url_for`].
//!
//! Both branches panic on an unregistered name so missing wiring is
//! caught during dev/test rather than producing broken links silently.

use reinhardt::ClientUrlResolver;

#[cfg(wasm)]
use reinhardt::pages::with_router;

#[cfg(not(wasm))]
use crate::client::route_table;

/// SPA URL resolver backed by either the globally installed
/// `pages::Router` (WASM) or the static [`route_table::SPA_ROUTES`]
/// table (native).
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

	// Native build: look up the static route table. Used for SSR `href`
	// generation and any server-side caller of `url_for(name)`.
	//
	// Workaround for kent8192/reinhardt-web#4067 (tracked in #501).
	// Once upstream lands a `Sync`-compatible `ClientRouter` (or splits
	// the `client-router` feature), `make_router` can register routes via
	// `UnifiedRouter::client(...)` and this branch should delegate to
	// `reinhardt::get_client_reverser()` instead. See route_table.rs for
	// the full ideal-implementation snippet.
	#[cfg(not(wasm))]
	fn resolve_client_url(&self, name: &str, params: &[(&str, &str)]) -> String {
		// Current routes are param-less; if a route gains `{param}`
		// placeholders, extend this lookup to perform substitution.
		assert!(
			params.is_empty(),
			"native SPA URL resolver does not yet support path parameters; \
			 route '{name}' was called with {} param(s). Implement substitution \
			 in route_table::lookup or wait for reinhardt-web#4067.",
			params.len()
		);
		match route_table::lookup(name) {
			Some(pattern) => pattern.to_string(),
			None => panic!(
				"SPA route '{name}' not registered in route_table::SPA_ROUTES; \
				 add it there and in init_router"
			),
		}
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

#[cfg(all(test, not(wasm)))]
mod tests {
	use super::{DashboardUrlResolver, url_for};
	use reinhardt::ClientUrlResolver;
	use rstest::rstest;

	// Verifies the native (non-WASM) `DashboardUrlResolver` produces the
	// path defined in `route_table::SPA_ROUTES`. Required for SSR `href`
	// generation. See kent8192/reinhardt-cloud#498 for context.
	#[rstest]
	#[case::home("dashboard:home", "/")]
	#[case::login("auth:login_page", "/login")]
	#[case::register("auth:register_page", "/register")]
	#[case::clusters("dashboard:clusters", "/clusters")]
	#[case::deployments("dashboard:deployments", "/deployments")]
	fn native_resolver_returns_pattern_from_route_table(
		#[case] name: &str,
		#[case] expected_path: &str,
	) {
		// Arrange
		let resolver = DashboardUrlResolver;

		// Act
		let resolved = resolver.resolve_client_url(name, &[]);

		// Assert
		assert_eq!(
			resolved, expected_path,
			"native DashboardUrlResolver must reverse-resolve '{name}' to '{expected_path}' \
			 so server-side SSR can produce correct hrefs"
		);
	}

	#[rstest]
	fn url_for_helper_resolves_login_page() {
		// Arrange & Act
		let url = url_for("auth:login_page");

		// Assert — guards against regressions in the convenience wrapper
		// when the native resolver branch changes.
		assert_eq!(url, "/login");
	}

	#[rstest]
	#[should_panic(expected = "not registered in route_table::SPA_ROUTES")]
	fn native_resolver_panics_on_unregistered_name() {
		// Arrange
		let resolver = DashboardUrlResolver;

		// Act — must panic so missing wiring is caught during dev/test
		// rather than producing broken links silently.
		let _ = resolver.resolve_client_url("nonexistent:route", &[]);
	}

	#[rstest]
	#[should_panic(expected = "does not yet support path parameters")]
	fn native_resolver_panics_when_params_are_passed() {
		// Arrange
		let resolver = DashboardUrlResolver;

		// Act — guard the explicit assertion until parameter substitution
		// is implemented (or upstream reinhardt-web#4067 lands).
		let _ = resolver.resolve_client_url("auth:login_page", &[("id", "1")]);
	}
}
