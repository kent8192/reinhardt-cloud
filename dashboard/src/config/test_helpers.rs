//! Test helpers for dashboard end-to-end tests.
//!
//! Provides the [`test_app`] fixture which resolves the dashboard router
//! from the DI registry and returns an in-process [`APIClient`] that
//! dispatches requests directly to the `Handler` without TCP.
//!
//! URL paths are resolved via `ServerRouter::reverse()` at construction time,
//! so tests are robust against path changes as long as route names stay the same.

use std::sync::Arc;

use reinhardt::OpenApiRouter;
use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::test::APIClient;
use rstest::fixture;

use crate::config::urls::{AllowedOrigins, DashboardRouter};

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
	pub auth_forgot_password: String,
	pub cluster_list: String,
	pub deployment_list: String,
}

impl TestUrls {
	/// Build the verify-email URL for a given token.
	pub fn auth_verify_email(&self, token: &str) -> String {
		format!("/api/auth/verify-email/{token}/")
	}

	/// Build the reset-password URL for a given token.
	pub fn auth_reset_password(&self, token: &str) -> String {
		format!("/api/auth/reset-password/{token}/")
	}
}

/// Build the dashboard router via DI and return an in-process test client
/// together with pre-resolved URL paths.
///
/// `AllowedOrigins` is pre-registered with `"http://testserver"` so the
/// `OriginGuardMiddleware` accepts requests from `APIClient::from_handler`.
/// All other singletons (`WsBroadcaster`, `LocalAuthService`,
/// `DashboardSessionConfig`) are resolved lazily via their
/// `#[injectable_factory]` registrations.
#[fixture]
pub fn test_app() -> (APIClient, TestUrls) {
	let scope = Arc::new(SingletonScope::new());
	scope.set(AllowedOrigins(vec!["http://testserver".into()]));
	let di_ctx = Arc::new(InjectionContext::builder(scope).build());

	let router: Arc<DashboardRouter> = tokio::task::block_in_place(|| {
		tokio::runtime::Handle::current().block_on(di_ctx.resolve::<DashboardRouter>())
	})
	.expect("Failed to resolve DashboardRouter");
	let server_router = Arc::try_unwrap(router)
		.expect("DashboardRouter has multiple owners after resolve")
		.0
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
		auth_forgot_password: rev("auth_forgot_password"),
		cluster_list: rev("cluster_list"),
		deployment_list: rev("deployment_list"),
	};

	// Wrap with OpenApiRouter to serve /api/openapi.json, /api/docs, /api/redoc.
	// In production this is done by the `runserver` command; tests must do it explicitly.
	let handler = OpenApiRouter::wrap(server_router).expect("Failed to wrap with OpenApiRouter");
	let client = APIClient::from_handler(handler);
	(client, urls)
}
