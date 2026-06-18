//! URL configuration for Reinhardt Cloud.
//!
//! The `routes` function defines all URL patterns for this project,
//! including server function registrations for the WASM frontend.
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
//! `#[injectable]`. These are aggregated into
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
	ContextLevel, Depends, DiRegistrationList, FactoryOutput, InjectionContext, get_di_context,
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
use crate::apps::auth::services::{LocalAuthService, LocalAuthServiceKey};
#[cfg(native)]
use crate::apps::auth::{server_fn, urls as auth_urls};
#[cfg(native)]
use crate::apps::clusters::{server_fn as cluster_server_fn, urls as cluster_urls};
#[cfg(native)]
use crate::apps::dashboard::urls as dashboard_urls;
#[cfg(native)]
use crate::apps::deployments::{server_fn as deployment_server_fn, urls as deployment_urls};
#[cfg(native)]
use crate::apps::github::{server_fn as github_server_fn, urls as github_urls};
#[cfg(native)]
use crate::apps::health::urls as health_urls;
#[cfg(native)]
use crate::apps::organizations::urls as organization_urls;
#[cfg(native)]
use crate::config::admin::configure_admin;
#[cfg(native)]
use crate::config::middleware::CspPathMiddleware;
#[cfg(native)]
use crate::config::settings::{get_redis_url, get_settings};
#[cfg(native)]
use crate::config::{GrpcChannelSingleton, GrpcChannelSingletonKey};
#[cfg(native)]
use crate::utils::realtime::{WsBroadcaster, WsBroadcasterKey};
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

#[cfg(native)]
#[reinhardt::di::injectable_key]
pub(crate) struct AllowedOriginsKey;

/// DI factory — resolves allowed origins from settings.
#[cfg(native)]
#[reinhardt::di::injectable(scope = "singleton")]
async fn create_allowed_origins() -> FactoryOutput<AllowedOriginsKey, AllowedOrigins> {
	let settings = get_settings();
	let port = std::env::var("PORT").ok();
	FactoryOutput::new(AllowedOrigins(build_allowed_origins(
		&settings.cors.allow_origins,
		settings.core.debug,
		port.as_deref(),
	)))
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

#[cfg(native)]
#[reinhardt::di::injectable_key]
pub(crate) struct DashboardSessionConfigKey;

/// DI factory — builds `DashboardSessionConfig` from settings.
#[cfg(native)]
#[reinhardt::di::injectable(scope = "singleton")]
async fn create_cookie_session_config()
-> FactoryOutput<DashboardSessionConfigKey, DashboardSessionConfig> {
	let settings = get_settings();
	FactoryOutput::new(DashboardSessionConfig(CookieSessionConfig {
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
	}))
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

#[cfg(native)]
#[reinhardt::di::injectable_key]
pub(crate) struct RouterInfrastructureKey;

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
#[reinhardt::di::injectable(scope = "transient")]
async fn create_router_infrastructure(
	#[inject] allowed_origins: Depends<AllowedOriginsKey, AllowedOrigins>,
	#[inject] session_config: Depends<DashboardSessionConfigKey, DashboardSessionConfig>,
	#[inject] _ws_broadcaster: Depends<WsBroadcasterKey, WsBroadcaster>,
	#[inject] _local_auth_service: Depends<LocalAuthServiceKey, LocalAuthService>,
	#[inject] _grpc_channel: Depends<GrpcChannelSingletonKey, GrpcChannelSingleton>,
) -> FactoryOutput<RouterInfrastructureKey, RouterInfrastructure> {
	let di_ctx = get_di_context(ContextLevel::Root);
	initialize_dashboard_static_resolver();

	// Configure admin site with DI registration.
	let admin_site = Arc::new(configure_admin());
	let (admin_router, admin_di) = admin_routes_with_di(admin_site);

	// Configure Redis-backed session backend.
	let redis_url = get_redis_url()
		.expect("Redis URL must be configured: set REINHARDT_CLOUD_REDIS_URL env var or redis_url in settings TOML");
	let session_backend = Arc::new(
		RedisSessionBackend::new_from_url(&redis_url)
			.expect("Failed to create Redis session backend"),
	);

	FactoryOutput::new(RouterInfrastructure {
		di_ctx,
		admin_router,
		admin_di,
		session_backend,
		allowed_origins: allowed_origins.0.clone(),
		session_config: session_config.0.clone(),
	})
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

#[cfg(native)]
#[reinhardt::di::injectable_key]
#[derive(Debug)]
pub(crate) struct DashboardRouterKey;

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
	router: Depends<DashboardRouterKey, DashboardRouter>,
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
#[reinhardt::di::injectable(scope = "transient")]
async fn make_router(
	#[inject] infra: Depends<RouterInfrastructureKey, RouterInfrastructure>,
) -> FactoryOutput<DashboardRouterKey, DashboardRouter> {
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
			.mount_unified("/", dashboard_urls::url_patterns())
			.mount_unified("/auth/", auth_urls::url_patterns())
			// Cluster and deployment data mutations are exposed through
			// registered server functions; the unified app mounts only
			// contribute SPA client routes.
			.mount_unified("/", cluster_urls::url_patterns())
			.mount_unified("/", deployment_urls::url_patterns())
			.mount_unified("/github/", github_urls::url_patterns())
			.mount_unified("/", health_urls::url_patterns())
			.mount_unified("/", organization_urls::url_patterns())
			.server(|s| {
				s.server_fn(server_fn::login::login::marker)
					.server_fn(server_fn::linked_accounts::list_linked_oauth_accounts::marker)
					.server_fn(server_fn::register::register::marker)
					.server_fn(server_fn::logout::logout::marker)
					.server_fn(server_fn::me::me::marker)
					.server_fn(server_fn::oauth_providers::list_oauth_providers::marker)
					.server_fn(cluster_server_fn::list_clusters_for_current_org::marker)
					.server_fn(cluster_server_fn::create_cluster_for_current_org::marker)
					.server_fn(cluster_server_fn::update_cluster_for_current_org::marker)
					.server_fn(cluster_server_fn::delete_cluster_for_current_org::marker)
					.server_fn(cluster_server_fn::rotate_cluster_token_for_current_org::marker)
					.server_fn(deployment_server_fn::list_deployments_for_current_org::marker)
					.server_fn(deployment_server_fn::create_deployment_for_current_org::marker)
					.server_fn(deployment_server_fn::update_deployment_for_current_org::marker)
					.server_fn(deployment_server_fn::delete_deployment_for_current_org::marker)
					.server_fn(deployment_server_fn::update_deployment_status_for_current_org::marker)
					.server_fn(deployment_server_fn::deployment_logs_for_current_org::marker)
					.server_fn(github_server_fn::get_github_onboarding_for_current_org::marker)
					.server_fn(github_server_fn::list_github_installations_for_current_org::marker)
					.server_fn(github_server_fn::list_github_repositories_for_current_org::marker)
					.server_fn(github_server_fn::list_github_repositories_for_installation::marker)
					.server_fn(github_server_fn::import_github_repository_for_current_org::marker)
			})
			.with_di_context(infra.di_ctx)
			.with_middleware(SecurityMiddleware::new())
			.with_middleware(CspPathMiddleware)
			.with_middleware(OriginGuardMiddleware::new(infra.allowed_origins))
			.with_middleware(crate::apps::auth::middleware::api_token::ApiTokenAuthMiddleware)
			.with_middleware(CookieSessionAuthMiddleware::with_config(
				infra.session_backend,
				infra.session_config,
			));

	FactoryOutput::new(DashboardRouter(unified))
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
