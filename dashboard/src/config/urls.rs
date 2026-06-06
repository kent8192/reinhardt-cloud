//! URL configuration for Reinhardt Cloud.
//!
//! The `routes` function defines all URL patterns for this project,
//! including REST API endpoints and server function registrations
//! for the WASM frontend.
//!
//! ## Cross-target compilation
//!
//! This module compiles on both native and wasm. On native it carries
//! the full router build (DI factories, admin routes, middleware,
//! WebSocket setup). On wasm only the `#[routes]`-decorated `routes()`
//! stub remains, which is sufficient for the macro to emit the URL
//! resolver support used by SPA call sites.
//!
//! ## DI-based configuration (native only)
//!
//! `AllowedOrigins`, `DashboardSessionConfig`, `WsBroadcaster`, and
//! `LocalAuthService` are auto-registered as singletons via
//! `#[injectable_factory]`. These are aggregated into
//! `RouterInfrastructure` (a transient factory) together with the
//! admin routes, DI context, and Redis session backend.
//! The router builder resolves `RouterInfrastructure` from the
//! DI registry at startup.
//!
//! ## WebSocket routes (native only)
//!
//! WebSocket route registration uses `WebSocketRouter` from
//! reinhardt, which is async and independent of `UnifiedRouter`.
//! See `init_websocket_routes()` below.

#[cfg(native)]
use std::sync::Arc;

#[cfg(native)]
use reinhardt::ServerRouter;
#[cfg(native)]
use reinhardt::admin::{admin_routes_with_di, admin_static_routes};
#[cfg(native)]
use reinhardt::di::{
	ContextLevel, Depends, DiRegistrationList, InjectionContext, get_di_context, injectable_factory,
};
#[cfg(native)]
use reinhardt::pages::server_fn::ServerFnRouterExt;
use reinhardt::routes;
#[cfg(native)]
use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use crate::shared::client::pages::not_found::not_found_page;

#[cfg(not(target_arch = "wasm32"))]
use reinhardt::{WebSocketRoute, WebSocketRouter, register_websocket_router};

#[cfg(native)]
use crate::apps::auth::server;
#[cfg(native)]
use crate::apps::auth::services::local_auth::LocalAuthService;
#[cfg(native)]
use crate::config::grpc_client::GrpcChannelSingleton;
#[cfg(native)]
use crate::config::middleware::CspPathMiddleware;
#[cfg(native)]
use crate::utils::realtime::broadcaster::WsBroadcaster;
#[cfg(native)]
use reinhardt::{
	CookieSessionAuthMiddleware, CookieSessionConfig, OriginGuardMiddleware, RedisSessionBackend,
	SecurityMiddleware,
};

#[cfg(native)]
const DASHBOARD_STATIC_URL_PREFIX: &str = "/api/static/";

// ── DI-registered singletons ────────────────────────────────────────

/// Allowed origins for the `OriginGuardMiddleware`.
///
/// Tests can override by pre-registering in `SingletonScope`
/// before the factory is invoked.
#[cfg(native)]
pub(crate) struct AllowedOrigins(pub Vec<String>);

/// DI factory — resolves allowed origins from settings.
#[cfg(native)]
#[injectable_factory(scope = "singleton")]
async fn create_allowed_origins() -> AllowedOrigins {
	let settings = crate::config::settings::get_settings();
	let port = std::env::var("PORT").ok();
	AllowedOrigins(build_allowed_origins(
		&settings.cors.allow_origins,
		settings.core.debug,
		port.as_deref(),
	))
}

#[cfg(native)]
fn build_allowed_origins(configured: &[String], debug: bool, port: Option<&str>) -> Vec<String> {
	let mut origins = Vec::new();
	for origin in configured {
		if origin != "*" && !origins.contains(origin) {
			origins.push(origin.clone());
		}
	}

	if debug {
		let port = port.unwrap_or("8000");
		for origin in [
			format!("http://localhost:{port}"),
			format!("http://127.0.0.1:{port}"),
		] {
			if !origins.contains(&origin) {
				origins.push(origin);
			}
		}
	}

	origins
}

/// Application-specific cookie session configuration.
///
/// Wraps the framework's `CookieSessionConfig` to comply with the
/// DI pseudo orphan rule (kent8192/reinhardt-web#3468). Factories
/// must not return framework-managed types directly.
#[cfg(native)]
#[derive(Debug)]
pub(crate) struct DashboardSessionConfig(pub CookieSessionConfig);

/// DI factory — builds `DashboardSessionConfig` from settings.
#[cfg(native)]
#[injectable_factory(scope = "singleton")]
async fn create_cookie_session_config() -> DashboardSessionConfig {
	let settings = crate::config::settings::get_settings();
	DashboardSessionConfig(CookieSessionConfig {
		cookie_name: "sessionid".to_string(),
		sliding_ttl: std::time::Duration::from_secs(1800),
		absolute_max: std::time::Duration::from_secs(86400),
		secure: !settings.core.debug,
		same_site: "Lax".to_string(),
		skip_paths: vec![
			"/api/auth/".to_string(),
			"/api/healthz/".to_string(),
			"/api/openapi.json".to_string(),
			"/api/docs".to_string(),
			"/api/redoc".to_string(),
		],
	})
}

// ── Router infrastructure ───────────────────────────────────────────

/// Pre-built infrastructure components for the application router.
///
/// Groups the DI context, admin routes with DI registrations,
/// session backend, and middleware configuration so they can be
/// resolved as a single dependency by `make_router`.
#[cfg(native)]
pub(crate) struct RouterInfrastructure {
	pub di_ctx: Arc<InjectionContext>,
	pub admin_router: ServerRouter,
	pub admin_di: DiRegistrationList,
	pub session_backend: Arc<RedisSessionBackend>,
	pub allowed_origins: Vec<String>,
	pub session_config: CookieSessionConfig,
}

/// DI factory — builds shared router infrastructure components.
///
/// Resolves `AllowedOrigins`, `DashboardSessionConfig`, `WsBroadcaster`,
/// `LocalAuthService`, and `GrpcChannelSingleton` from the DI registry,
/// then constructs the DI context, admin site routes, and Redis session
/// backend. `WsBroadcaster`, `LocalAuthService`, and `GrpcChannelSingleton`
/// are injected solely to trigger their singleton initialization at startup
/// — this surfaces a misconfigured `GRPC_ENDPOINT` immediately rather than
/// on the first RPC.
#[cfg(native)]
#[injectable_factory(scope = "transient")]
async fn create_router_infrastructure(
	#[inject] allowed_origins: Depends<AllowedOrigins>,
	#[inject] session_config: Depends<DashboardSessionConfig>,
	#[inject] _ws_broadcaster: Depends<WsBroadcaster>,
	#[inject] _local_auth_service: Depends<LocalAuthService>,
	#[inject] _grpc_channel: Depends<GrpcChannelSingleton>,
) -> RouterInfrastructure {
	let di_ctx = get_di_context(ContextLevel::Root);
	initialize_dashboard_static_resolver();

	// Configure admin site with DI registration.
	let admin_site = Arc::new(crate::config::admin::configure_admin());
	let (admin_router, admin_di) = admin_routes_with_di(admin_site);

	// Configure Redis-backed session backend.
	let redis_url = crate::config::settings::get_redis_url()
		.expect("Redis URL must be configured: set REINHARDT_CLOUD_REDIS_URL env var or redis_url in settings TOML");
	let session_backend = Arc::new(
		RedisSessionBackend::new_from_url(&redis_url)
			.expect("Failed to create Redis session backend"),
	);

	RouterInfrastructure {
		di_ctx,
		admin_router,
		admin_di,
		session_backend,
		allowed_origins: allowed_origins.0.clone(),
		session_config: session_config.0.clone(),
	}
}

// ── Router construction ─────────────────────────────────────────────

/// Application-specific router wrapper.
///
/// Wraps the framework's `UnifiedRouter` to comply with the DI pseudo
/// orphan rule (kent8192/reinhardt-web#3468). The `#[routes]` entry
/// point unwraps this to return the inner `UnifiedRouter` to the framework.
#[cfg(native)]
#[derive(Debug)]
pub(crate) struct DashboardRouter(pub UnifiedRouter);

/// Entry point for the `#[routes]` macro (called by the framework).
///
/// On native, the `#[inject]` parameter resolves `DashboardRouter` from
/// the DI registry, which triggers the `make_router` factory and all its
/// transitive dependencies. The framework creates the `InjectionContext`
/// automatically for async routes.
///
/// On wasm the function reduces to a stub that returns an empty
/// `UnifiedRouter`. The body is never executed in a browser context —
/// the macro only needs the function to exist so it can emit URL
/// resolver support adjacent to it.
#[routes]
#[allow(private_interfaces)] // DashboardRouter is pub(crate) by design; #[routes] macro requires pub fn
pub async fn routes(
	#[cfg(native)]
	#[inject]
	router: Depends<DashboardRouter>,
) -> UnifiedRouter {
	#[cfg(native)]
	{
		router
			.try_unwrap()
			.expect("DashboardRouter has multiple owners after resolve")
			.0
	}
	#[cfg(not(native))]
	{
		UnifiedRouter::new()
	}
}

/// Initialize URL generation for assets served by the dashboard router.
///
/// The project router is mounted under `/api/`, so the admin shell must emit
/// `/api/static/admin/...` URLs for assets served by `admin_static_routes()`.
#[cfg(native)]
fn initialize_dashboard_static_resolver() {
	reinhardt::pages::init_static_resolver(
		reinhardt::utils::staticfiles::TemplateStaticConfig::new(
			DASHBOARD_STATIC_URL_PREFIX.to_string(),
		),
	);
}

/// Build the application router by resolving dependencies from DI.
///
/// Infrastructure components (`RouterInfrastructure`) are resolved
/// transitively from the DI registry. Tests can override singletons
/// like `AllowedOrigins` by pre-registering in the `SingletonScope`
/// before calling this function.
#[cfg(native)]
#[injectable_factory(scope = "transient")]
async fn make_router(#[inject] infra: Depends<RouterInfrastructure>) -> DashboardRouter {
	let infra = infra
		.try_unwrap()
		.unwrap_or_else(|_| panic!("RouterInfrastructure has multiple owners after resolve"));

	let unified = UnifiedRouter::new()
			// Project-level SPA 404 fallback — owned here rather than by any
			// individual app because Reinhardt's `not_found` slot is per
			// `UnifiedRouter`, not per mounted segment.
			.client(|c| c.not_found(not_found_page))
			// Admin panel
			.mount("/admin/", infra.admin_router)
			.mount("/static/admin/", admin_static_routes())
			.with_prefix("/api/")
			.with_di_registrations(infra.admin_di)
			// Per-app unified routers — `mount_unified` carries server
			// endpoints (mounted under the given prefix) AND merges client
			// SPA `route` entries into the parent's client router so
			// the global reverser sees `auth:login_page`,
			// `dashboard:home`, etc.
			.mount_unified("/", crate::apps::dashboard::urls::url_patterns())
			.mount_unified("/auth/", crate::apps::auth::urls::url_patterns())
			// Mount at "/" and embed the full `/orgs/{org}/...` path in each
			// view macro. This is intentional: parameter-prefixed mount
			// (e.g. `.mount_unified("/orgs/{org}/", ...)`) would create an
			// implicit, non-local URL contract — a view's accepted path
			// params would depend on which mount it was attached to,
			// breaking locality and refactor safety. Upstream surfaces the
			// mistake loudly via kent8192/reinhardt-web#4012 (PR #4015
			// panics on `{` / `}` in the prefix); the broader feature was
			// rejected on design grounds in kent8192/reinhardt-web#4023.
			.mount_unified("/", crate::apps::clusters::urls::url_patterns())
			.mount_unified("/", crate::apps::deployments::urls::url_patterns())
			// Deprecated flat-URL redirects: 307 to org-scoped URL (removed after next release)
			.mount(
				"/clusters/",
				crate::config::middleware::deprecated_flat_urls::clusters_redirect_patterns(),
			)
			.mount(
				"/deployments/",
				crate::config::middleware::deprecated_flat_urls::deployments_redirect_patterns(),
			)
			.mount_unified("/", crate::apps::health::urls::url_patterns())
			.mount_unified("/", crate::apps::organizations::urls::url_patterns())
			.server(|s| {
				s.server_fn(server::login::login::marker)
					.server_fn(server::register::register::marker)
					.server_fn(server::logout::logout::marker)
					.server_fn(server::me::me::marker)
					.server_fn(server::oauth_providers::list_oauth_providers::marker)
					.server_fn(crate::apps::clusters::server::list_clusters_for_current_org::marker)
					.server_fn(crate::apps::clusters::server::create_cluster_for_current_org::marker)
					.server_fn(crate::apps::clusters::server::update_cluster_for_current_org::marker)
					.server_fn(crate::apps::clusters::server::delete_cluster_for_current_org::marker)
					.server_fn(crate::apps::clusters::server::rotate_cluster_token_for_current_org::marker)
					.server_fn(crate::apps::deployments::server::list_deployments_for_current_org::marker)
					.server_fn(crate::apps::deployments::server::create_deployment_for_current_org::marker)
					.server_fn(crate::apps::deployments::server::update_deployment_for_current_org::marker)
					.server_fn(crate::apps::deployments::server::delete_deployment_for_current_org::marker)
					.server_fn(crate::apps::deployments::server::update_deployment_status_for_current_org::marker)
					.server_fn(crate::apps::deployments::server::deployment_logs_for_current_org::marker)
			})
			.with_di_context(infra.di_ctx)
			.with_middleware(SecurityMiddleware::new())
			.with_middleware(CspPathMiddleware)
			.with_middleware(OriginGuardMiddleware::new(infra.allowed_origins))
			.with_middleware(CookieSessionAuthMiddleware::with_config(
				infra.session_backend,
				infra.session_config,
			));

	DashboardRouter(unified)
}

// ── WebSocket routes ────────────────────────────────────────────────

/// Initialize WebSocket routes.
///
/// Registers the `/ws/notifications` endpoint for real-time event delivery
/// to connected dashboard clients. This function must be called during
/// server startup, independently of the URL router configuration.
#[cfg(not(target_arch = "wasm32"))]
pub async fn init_websocket_routes() {
	let mut ws_router = WebSocketRouter::new();
	let route = WebSocketRoute::new(
		"/ws/notifications".to_string(),
		Some("websocket:notifications".to_string()),
	);
	ws_router
		.register_route(route)
		.await
		.expect("failed to register /ws/notifications route");
	register_websocket_router(ws_router).await;
}

#[cfg(all(test, native))]
mod tests {
	use serial_test::serial;

	use super::*;

	#[test]
	#[serial(admin_static_resolver)]
	fn admin_static_assets_resolve_under_api_static_prefix() {
		// Arrange
		initialize_dashboard_static_resolver();

		// Act
		let url = reinhardt::pages::resolve_static("admin/main.js");

		// Assert
		assert_eq!(url, "/api/static/admin/main.js");
	}

	#[rstest::rstest]
	fn debug_allowed_origins_include_configured_and_active_port() {
		// Arrange
		let configured = vec![
			"http://localhost:8000".to_string(),
			"http://127.0.0.1:8000".to_string(),
		];

		// Act
		let origins = build_allowed_origins(&configured, true, Some("8001"));

		// Assert
		assert_eq!(
			origins,
			vec![
				"http://localhost:8000".to_string(),
				"http://127.0.0.1:8000".to_string(),
				"http://localhost:8001".to_string(),
				"http://127.0.0.1:8001".to_string(),
			]
		);
	}

	#[rstest::rstest]
	fn debug_allowed_origins_fall_back_when_configured_only_wildcard() {
		// Arrange
		let configured = vec!["*".to_string()];

		// Act
		let origins = build_allowed_origins(&configured, true, Some("8001"));

		// Assert
		assert_eq!(
			origins,
			vec![
				"http://localhost:8001".to_string(),
				"http://127.0.0.1:8001".to_string(),
			]
		);
	}

	#[rstest::rstest]
	fn production_allowed_origins_do_not_add_localhost_fallbacks() {
		// Arrange
		let configured = vec!["https://reinhardt-cloud.dev".to_string()];

		// Act
		let origins = build_allowed_origins(&configured, false, Some("8001"));

		// Assert
		assert_eq!(origins, vec!["https://reinhardt-cloud.dev".to_string()]);
	}
}
