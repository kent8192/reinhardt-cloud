//! Test helpers for dashboard end-to-end tests.
//!
//! Provides the [`test_app`] fixture which resolves the dashboard router
//! from the DI registry and returns an in-process [`APIClient`] that
//! dispatches requests directly to the `Handler` without TCP.
//!
//! URL paths are resolved via `ServerRouter::reverse()` at construction time,
//! so tests are robust against path changes as long as route names stay the same.

use std::sync::Arc;

use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::test::APIClient;
use rstest::fixture;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::config::urls::AllowedOrigins;

/// Pre-resolved URL paths for test use.
///
/// Built from `ServerRouter::reverse()` at construction time so that
/// tests break at the right place if a route name is removed, and
/// automatically pick up path changes without manual updates.
pub struct TestUrls {
	pub auth_register: String,
	pub auth_login: String,
	pub auth_profile: String,
	pub auth_change_password: String,
	pub cluster_list: String,
	pub deployment_list: String,
}

/// Build the dashboard router via DI and return an in-process test client
/// together with pre-resolved URL paths.
///
/// `AllowedOrigins` is pre-registered with `"http://testserver"` so the
/// `OriginGuardMiddleware` accepts requests from `APIClient::from_handler`.
/// All other singletons (`WsBroadcaster`, `LocalAuthService`,
/// `CookieSessionConfig`) are resolved lazily via their
/// `#[injectable_factory]` registrations.
#[fixture]
pub fn test_app() -> (APIClient, TestUrls) {
	let scope = Arc::new(SingletonScope::new());
	scope.set(AllowedOrigins(vec!["http://testserver".into()]));
	let di_ctx = Arc::new(InjectionContext::builder(scope).build());

	let router: Arc<UnifiedRouter> = tokio::task::block_in_place(|| {
		tokio::runtime::Handle::current().block_on(di_ctx.resolve::<UnifiedRouter>())
	})
	.expect("Failed to resolve UnifiedRouter");
	let server_router = Arc::try_unwrap(router)
		.expect("UnifiedRouter has multiple owners after resolve")
		.into_server();

	let rev = |name: &str| -> String {
		let url = server_router
			.reverse(name, &[])
			.unwrap_or_else(|| panic!("Route '{name}' not found in router"));
		eprintln!("[test_app] reverse(\"{name}\") => \"{url}\"");
		url
	};

	let urls = TestUrls {
		auth_register: rev("auth_register"),
		auth_login: rev("auth_login"),
		auth_profile: rev("auth_profile"),
		auth_change_password: rev("auth_change_password"),
		cluster_list: rev("cluster_list"),
		deployment_list: rev("deployment_list"),
	};

	let client = APIClient::from_handler(server_router);
	(client, urls)
}
