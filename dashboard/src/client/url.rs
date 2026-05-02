//! Client-side URL resolution for the SPA.
//!
//! Provides a [`ClientUrlResolver`] implementation that resolves named
//! SPA routes (e.g. `auth:login_page`) to URL paths. SPA components call
//! [`urls()`] to look up routes registered in
//! `crate::config::urls::make_router` (server-side, via
//! `UnifiedRouter::client(...)`) and `super::router::init_router` (WASM
//! side, for the SPA `Router`).
//!
//! Route name conventions follow the server-side `<app>:<name>` pattern.
//!
//! # Cross-target behaviour
//!
//! - **WASM**: delegates to the globally installed `pages::Router`
//!   (built by [`super::router::init_router`]) via
//!   `pages::with_router(|r| r.reverse(name, params))`.
//! - **Native**: delegates to the global [`ClientUrlReverser`] registered
//!   by `make_router` calling
//!   `register_client_reverser(unified.client_ref().to_reverser())`.
//!   Required for SSR `href` generation.
//!
//! Both branches panic on an unregistered name so missing wiring is
//! caught during dev/test rather than producing broken links silently.

use reinhardt::ClientUrlResolver;

#[cfg(wasm)]
use reinhardt::pages::with_router;

#[cfg(not(wasm))]
use reinhardt::get_client_reverser;

/// SPA URL resolver backed by either the globally installed
/// `pages::Router` (WASM) or the globally registered
/// `ClientUrlReverser` (native).
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

	// Native build: delegate to the globally registered
	// `ClientUrlReverser`. Registration happens inside
	// `make_router` via
	// `register_client_reverser(unified.client_ref().to_reverser())`,
	// which is invoked when the DI container resolves
	// `DashboardRouter` at startup (and at the start of every test
	// that resolves the router through DI).
	#[cfg(not(wasm))]
	fn resolve_client_url(&self, name: &str, params: &[(&str, &str)]) -> String {
		let reverser = get_client_reverser().unwrap_or_else(|| {
			panic!(
				"global ClientUrlReverser not registered; ensure DashboardRouter \
				 has been resolved through DI (which calls register_client_reverser \
				 inside make_router) before invoking url_for"
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
#[deprecated(
	since = "0.1.0",
	note = "Use crate::client::client_urls::<app>::<route>() instead — string-based \
	        SPA route lookup loses compile-time safety. Removal is tracked in a \
	        follow-up to kent8192/reinhardt-cloud#519."
)]
pub fn url_for(name: &str) -> String {
	urls().resolve_client_url(name, &[])
}

#[cfg(all(test, not(wasm)))]
// Tests cover the deprecated `url_for` alongside the resolver to ensure
// the deprecation does not silently break the existing wrapper. Removal
// of `url_for` in a follow-up PR will also drop these tests.
#[allow(deprecated)]
mod tests {
	use super::{DashboardUrlResolver, url_for};
	use crate::config::test_helpers::build_test_app;
	use reinhardt::ClientUrlResolver;
	use rstest::rstest;
	use serial_test::serial;

	// Verifies the native `DashboardUrlResolver` delegates to the
	// globally registered `ClientUrlReverser`. Required for SSR `href`
	// generation. See kent8192/reinhardt-cloud#498 + #501;
	// kent8192/reinhardt-web#4067 / #4068.
	#[rstest]
	#[case::home("dashboard:home", "/")]
	#[case::login("auth:login_page", "/login")]
	#[case::register("auth:register_page", "/register")]
	#[case::clusters("dashboard:clusters", "/clusters")]
	#[case::deployments("dashboard:deployments", "/deployments")]
	#[serial(global_client_reverser)]
	#[tokio::test(flavor = "multi_thread")]
	async fn native_resolver_delegates_to_global_reverser(
		#[case] name: &str,
		#[case] expected_path: &str,
	) {
		// Arrange — building the router through DI registers the global
		// reverser as a side effect of make_router.
		let _app = build_test_app();
		let resolver = DashboardUrlResolver;

		// Act
		let resolved = resolver.resolve_client_url(name, &[]);

		// Assert
		assert_eq!(
			resolved, expected_path,
			"native DashboardUrlResolver must reverse-resolve '{name}' to '{expected_path}' \
			 via the globally registered ClientUrlReverser"
		);
	}

	#[rstest]
	#[serial(global_client_reverser)]
	#[tokio::test(flavor = "multi_thread")]
	async fn url_for_helper_resolves_login_page() {
		// Arrange
		let _app = build_test_app();

		// Act
		let url = url_for("auth:login_page");

		// Assert — guards against regressions in the convenience wrapper
		// when the native resolver branch changes.
		assert_eq!(url, "/login");
	}

	#[rstest]
	#[should_panic(expected = "not registered in make_router")]
	#[serial(global_client_reverser)]
	#[tokio::test(flavor = "multi_thread")]
	async fn native_resolver_panics_on_unregistered_name() {
		// Arrange
		let _app = build_test_app();
		let resolver = DashboardUrlResolver;

		// Act — must panic so missing wiring is caught during dev/test
		// rather than producing broken links silently.
		let _ = resolver.resolve_client_url("nonexistent:route", &[]);
	}
}
