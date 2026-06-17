//! Test helpers for dashboard end-to-end tests.
//!
//! Provides the [`test_app`] fixture which resolves the dashboard router
//! from the DI registry and returns an in-process [`APIClient`] that
//! dispatches requests directly to the `Handler` without TCP.
//!
//! URL paths are resolved via [`UrlReverser::from_global()`], so tests pick
//! up path changes automatically and route renames become runtime errors
//! caught by the `unwrap()` at each call site.

use std::sync::Arc;

use reinhardt::OpenApiRouter;
use reinhardt::RedisSessionBackend;
use reinhardt::di::{FactoryOutput, InjectionContext, SingletonScope};
use reinhardt::middleware::session::AsyncSessionBackend;
use reinhardt::prelude::DatabaseConnection;
use reinhardt::test::APIClient;
use reinhardt::{UrlReverser, register_router_arc};
use rstest::fixture;

/// Build a fresh `InjectionContext` for factory unit tests.
///
/// Centralizes the four-line setup (`SingletonScope::new` →
/// `scope.set` overrides → `InjectionContext::builder` → `Arc::new`)
/// used by services converted to keyed `#[injectable]` providers. The
/// closure parameter receives the scope before the context is built so
/// tests can override any dependency that the factory under test injects
/// via `Depends<Key, T>`.
///
/// # Examples
///
/// ```ignore
/// let ctx = make_test_di_context(|scope| {
///     scope.set(FactoryOutput::<MySettingsKey, MySettings>::new(MySettings {
///         jwt_secret: "test".into(),
///     }));
/// });
/// let svc: Arc<FactoryOutput<MyServiceKey, MyService>> =
///     ctx.resolve::<FactoryOutput<MyServiceKey, MyService>>().await.unwrap();
/// ```
pub fn make_test_di_context<F>(setup: F) -> Arc<InjectionContext>
where
	F: FnOnce(&Arc<SingletonScope>),
{
	let scope = Arc::new(SingletonScope::new());
	setup(&scope);
	Arc::new(InjectionContext::builder(scope).build())
}

use crate::apps::auth::models::User;
use crate::apps::organizations::models::{Organization, OrganizationMembership};
use crate::apps::organizations::roles::{MembershipRole, sanitize_username_to_slug};
use crate::config::settings::get_redis_url;
use crate::config::urls::{AllowedOrigins, AllowedOriginsKey, DashboardRouter, DashboardRouterKey};

/// Build the dashboard router via DI and return an in-process test client
/// together with the [`UrlReverser`] for URL reverse-resolution.
///
/// `AllowedOrigins` is pre-registered with `"http://testserver"` so the
/// `OriginGuardMiddleware` accepts requests from `APIClient::from_handler`.
/// All other singletons (`WsBroadcaster`, `LocalAuthService`,
/// `DashboardSessionConfig`) are resolved lazily via their
/// `#[injectable]` registrations.
///
/// If the global ORM has already been initialised (e.g. by a preceding
/// `postgres_with_migrations_from_dir` call that invoked
/// `reinitialize_database`), the resulting `DatabaseConnection` is also
/// registered in the `SingletonScope` so that view handlers that obtain a DB
/// connection via DI see the same
/// TestContainers database as `create_with_conn` helpers. When the global ORM
/// is not yet initialised (e.g. when `test_app` is constructed before the
/// TestContainers fixture runs), the `DatabaseConnection` singleton is
/// omitted; view handlers fall back to the global ORM connection, which will
/// be pointed at the TestContainers DB once `reinitialize_database` is called.
#[fixture]
pub fn test_app() -> (APIClient, Arc<UrlReverser>) {
	build_test_app()
}

/// Internal helper shared by [`test_app`] and other callers that need to build
/// the test app after a TestContainers database has been initialised.
///
/// Constructs the DI context and API client. If the global ORM connection is
/// available at call time (i.e. `reinitialize_database` has already been
/// called), the `DatabaseConnection` is registered in the `SingletonScope` so
/// that view handlers using DI see the
/// TestContainers database.
///
/// Exposed as `pub` so that `db` fixtures in individual test modules can call
/// it **after** `postgres_with_migrations_from_dir` has set up the global ORM,
/// ensuring the DI context holds the correct `DatabaseConnection` from the
/// start. Without this ordering, view handlers that use DI-based DB injection
/// would not see the
/// TestContainers database.
///
/// The returned [`UrlReverser`] supports `reverse_with(name, params)` for URL
/// resolution in tests. Registering the router mirrors production startup so
/// tests exercise the same global URL reversal path as application code.
pub fn build_test_app() -> (APIClient, Arc<UrlReverser>) {
	let scope = Arc::new(SingletonScope::new());
	scope.set(FactoryOutput::<AllowedOriginsKey, AllowedOrigins>::new(
		AllowedOrigins(vec!["http://testserver".into()]),
	));

	// Register the global DatabaseConnection in the DI scope when available.
	// This ensures view handlers see the same DB connection as helpers using
	// `create_with_conn`.
	tokio::task::block_in_place(|| {
		tokio::runtime::Handle::current().block_on(async {
			if let Ok(conn) = reinhardt::db::orm::get_connection().await {
				scope.set(conn);
			}
		})
	});

	let di_ctx = Arc::new(InjectionContext::builder(scope).build());

	let router: Arc<FactoryOutput<DashboardRouterKey, DashboardRouter>> =
		tokio::task::block_in_place(|| {
			tokio::runtime::Handle::current()
				.block_on(di_ctx.resolve::<FactoryOutput<DashboardRouterKey, DashboardRouter>>())
		})
		.expect("Failed to resolve DashboardRouter");

	let server_router = Arc::new(
		Arc::try_unwrap(router)
			.expect("DashboardRouter has multiple owners after resolve")
			.into_inner()
			.0
			.into_server(),
	);
	register_router_arc(server_router.clone());
	let url_reverser = UrlReverser::from_global();

	// Wrap with OpenApiRouter to serve /api/openapi.json, /api/docs, /api/redoc.
	// In production this is done by the `runserver` command; tests must do it explicitly.
	let handler =
		OpenApiRouter::wrap(server_router.clone()).expect("Failed to wrap with OpenApiRouter");
	let client = APIClient::from_handler(handler);
	(client, url_reverser)
}

/// Redis-backed session backend for force-login in tests.
///
/// Connects to the same Redis instance used by the application middleware,
/// so sessions saved here are visible to `CookieSessionAuthMiddleware`.
#[fixture]
pub fn session_backend() -> Arc<dyn AsyncSessionBackend> {
	let redis_url = get_redis_url().expect("Redis URL must be configured for session tests");
	Arc::new(
		RedisSessionBackend::new_from_url(&redis_url)
			.expect("Failed to create Redis session backend"),
	)
}

/// Create a test user in the database (with a Personal Organization +
/// Owner Membership) and force-login on the client.
///
/// Creates the user via ORM, provisions a Personal Org so that views
/// using `current_organization_id_for_user` succeed, then calls
/// [`force_login`] to establish a session.
///
/// Returns the created `User`. The caller can obtain the Personal Org slug
/// via `org.slug` from a separate call to
/// `provision_personal_org_for_user`, or use `force_login_user_with_org`
/// when the org slug is needed immediately.
///
/// The slug used for the Personal Org is derived from `username` via
/// `sanitize_username_to_slug` plus a uuid suffix to avoid collisions
/// when many tests share the same username.
pub async fn force_login_user(
	client: &APIClient,
	conn: &Arc<DatabaseConnection>,
	session_backend: &Arc<dyn AsyncSessionBackend>,
	username: &str,
	email: &str,
) -> User {
	use reinhardt::db::orm::Model;

	let user = User::build()
		.username(username.to_string())
		.email(email.to_string())
		.first_name(String::new())
		.last_name(String::new())
		.password_hash(None)
		.is_active(true)
		.is_staff(false)
		.is_superuser(false)
		.finish();
	let user = User::objects()
		.create_with_conn(conn, &user)
		.await
		.expect("Failed to create test user");

	provision_personal_org_for_user(conn, &user).await;

	force_login(client, session_backend, &user).await;
	user
}

/// Like [`force_login_user`], but also returns the Personal `Organization`.
///
/// Use this when the test needs the Personal Org identity for server
/// function input or direct database assertions.
pub async fn force_login_user_with_org(
	client: &APIClient,
	conn: &Arc<DatabaseConnection>,
	session_backend: &Arc<dyn AsyncSessionBackend>,
	username: &str,
	email: &str,
) -> (User, Organization) {
	use reinhardt::db::orm::Model;

	let user = User::build()
		.username(username.to_string())
		.email(email.to_string())
		.first_name(String::new())
		.last_name(String::new())
		.password_hash(None)
		.is_active(true)
		.is_staff(false)
		.is_superuser(false)
		.finish();
	let user = User::objects()
		.create_with_conn(conn, &user)
		.await
		.expect("Failed to create test user");

	let org = provision_personal_org_for_user(conn, &user).await;

	force_login(client, session_backend, &user).await;
	(user, org)
}

/// Provision a Personal `Organization` and an `Owner` `OrganizationMembership`
/// for an already-created test user. Returns the created `Organization` so
/// callers can use the slug for org-scoped API URL construction.
///
/// Mirrors the runtime behaviour of the registration view (see
/// `dashboard/src/apps/auth/server/register.rs`) so that e2e tests using
/// `User::objects().create_with_conn` still satisfy the invariant that
/// every user has at least one organization membership.
///
/// The slug uses a `<sanitized>-<short-uuid>` form to avoid collisions
/// when multiple tests register the same username (each test gets its
/// own DB container, but defensive uniqueness is cheap).
pub async fn provision_personal_org_for_user(
	conn: &Arc<DatabaseConnection>,
	user: &User,
) -> Organization {
	use reinhardt::db::orm::Model;

	let now = chrono::Utc::now();
	let suffix = uuid::Uuid::new_v4().simple().to_string();
	let slug = format!(
		"{}-{}",
		sanitize_username_to_slug(&user.username),
		&suffix[..6]
	);

	let org = Organization::objects()
		.create_with_conn(
			conn,
			&Organization {
				id: None,
				slug,
				name: user.username.clone(),
				created_by: user.id,
				created_at: now,
				updated_at: now,
			},
		)
		.await
		.expect("Failed to create Personal Org for test user");

	OrganizationMembership::objects()
		.create_with_conn(
			conn,
			&OrganizationMembership::build()
				.organization(org.id.expect("created Organization has id"))
				.user(user.id)
				.role(MembershipRole::Owner.as_db_str().to_string())
				.finish(),
		)
		.await
		.expect("Failed to create Owner membership for test user");

	org
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

/// Update an existing user's membership role in the org returned by
/// `current_organization_id_for_user` (see
/// `crate::apps::organizations::helpers`).
///
/// Used by RBAC integration tests to demote a user's Personal-Org Owner
/// membership to e.g. Viewer or Developer before exercising a write
/// endpoint. Panics if the user has no membership row — only call after
/// [`force_login_user`] / [`provision_personal_org_for_user`].
pub async fn set_membership_role(
	conn: &Arc<DatabaseConnection>,
	user: &User,
	role: MembershipRole,
) {
	use reinhardt::db::orm::Model;

	let mut membership = OrganizationMembership::objects()
		.filter(OrganizationMembership::field_user_id().eq(user.id.to_string()))
		.first()
		.await
		.expect("Failed to look up membership for set_membership_role")
		.expect("set_membership_role called for user with no membership");
	membership.role = role.as_db_str().to_string();
	OrganizationMembership::objects()
		.update_with_conn(conn, &membership)
		.await
		.expect("Failed to update membership role");
}
