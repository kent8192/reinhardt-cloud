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
use reinhardt::RedisSessionBackend;
use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::middleware::session::AsyncSessionBackend;
use reinhardt::prelude::DatabaseConnection;
use reinhardt::test::APIClient;
use rstest::fixture;

use crate::apps::auth::models::User;
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
	pub cluster_list: String,
	pub deployment_list: String,
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
		cluster_list: rev("cluster_list"),
		deployment_list: rev("deployment_list"),
	};

	// Wrap with OpenApiRouter to serve /api/openapi.json, /api/docs, /api/redoc.
	// In production this is done by the `runserver` command; tests must do it explicitly.
	let handler = OpenApiRouter::wrap(server_router).expect("Failed to wrap with OpenApiRouter");
	let client = APIClient::from_handler(handler);
	(client, urls)
}

/// Redis-backed session backend for force-login in tests.
///
/// Connects to the same Redis instance used by the application middleware,
/// so sessions saved here are visible to `CookieSessionAuthMiddleware`.
#[fixture]
pub fn session_backend() -> Arc<dyn AsyncSessionBackend> {
	let redis_url = crate::config::settings::get_redis_url()
		.expect("Redis URL must be configured for session tests");
	Arc::new(
		RedisSessionBackend::new_from_url(&redis_url)
			.expect("Failed to create Redis session backend"),
	)
}

/// Create a test user in the database and force-login on the client.
///
/// Creates the user via ORM, then calls [`force_login`] to establish
/// a session. Use this for initial user setup. For switching back to
/// an already-created user, use [`force_login`] directly.
pub async fn force_login_user(
	client: &APIClient,
	conn: &Arc<DatabaseConnection>,
	session_backend: &Arc<dyn AsyncSessionBackend>,
	username: &str,
	email: &str,
) -> User {
	use reinhardt::db::orm::Model;

	let user = User::new(
		username.to_string(),
		email.to_string(),
		String::new(),
		String::new(),
		None,
		true,
		false,
		false,
	);
	let user = User::objects()
		.create_with_conn(conn, &user)
		.await
		.expect("Failed to create test user");

	force_login(client, session_backend, &user).await;
	user
}

/// Force-login an existing user on the client.
///
/// Creates a new session in the Redis backend for the given user and sets
/// the `sessionid` cookie. Use this to switch the client to a different
/// user without creating a new database record.
pub async fn force_login(
	client: &APIClient,
	session_backend: &Arc<dyn AsyncSessionBackend>,
	user: &User,
) {
	client
		.auth()
		.session(user, session_backend.clone())
		.apply()
		.await
		.expect("Failed to force login");
}
