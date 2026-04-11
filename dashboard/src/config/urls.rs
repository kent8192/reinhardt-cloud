//! URL configuration for Reinhardt Cloud.
//!
//! The `routes` function defines all URL patterns for this project,
//! including REST API endpoints and server function registrations
//! for the WASM frontend.
//!
//! ## DI-based configuration
//!
//! `AllowedOrigins`, `CookieSessionConfig`, `WsBroadcaster`, and
//! `LocalAuthService` are auto-registered as singletons via
//! `#[injectable_factory]`. These are aggregated into
//! `RouterInfrastructure` (a transient factory) together with the
//! admin routes, DI context, and Redis session backend.
//! The router builder resolves `RouterInfrastructure` from the
//! DI registry at startup.
//!
//! ## WebSocket routes
//!
//! WebSocket route registration uses `WebSocketRouter` from
//! reinhardt, which is async and independent of `UnifiedRouter`.
//! See `init_websocket_routes()` below.

use std::sync::Arc;

use reinhardt::admin::{admin_routes_with_di, admin_static_routes};
use reinhardt::di::{
	ContextLevel, Depends, DiRegistrationList, InjectionContext, get_di_context,
	injectable_factory,
};
use reinhardt::pages::server_fn::ServerFnRouterExt;
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;
use reinhardt::ServerRouter;

#[cfg(not(target_arch = "wasm32"))]
use reinhardt::{WebSocketRoute, WebSocketRouter, register_websocket_router};

use crate::apps::auth::server;
use crate::apps::auth::services::local_auth::LocalAuthService;
use crate::apps::realtime::broadcaster::WsBroadcaster;
use crate::config::middleware::CspPathMiddleware;
use reinhardt::{
	CookieSessionAuthMiddleware, CookieSessionConfig, OriginGuardMiddleware, RedisSessionBackend,
	SecurityMiddleware,
};

// ── DI-registered singletons ────────────────────────────────────────

/// Allowed origins for the `OriginGuardMiddleware`.
///
/// Tests can override by pre-registering in `SingletonScope`
/// before the factory is invoked.
#[derive(Clone)]
pub(crate) struct AllowedOrigins(pub Vec<String>);

/// DI factory — resolves allowed origins from settings.
#[injectable_factory(scope = "singleton")]
async fn create_allowed_origins() -> AllowedOrigins {
	let settings = crate::config::settings::get_settings();
	let mut origins = settings.cors.allow_origins.clone();
	origins.retain(|o| o != "*");
	if origins.is_empty() {
		let port = std::env::var("PORT").unwrap_or("8000".to_string());
		AllowedOrigins(vec![
			format!("http://localhost:{port}"),
			format!("http://127.0.0.1:{port}"),
		])
	} else {
		AllowedOrigins(origins)
	}
}

/// DI factory — builds `CookieSessionConfig` from settings.
#[injectable_factory(scope = "singleton")]
async fn create_cookie_session_config() -> CookieSessionConfig {
	let settings = crate::config::settings::get_settings();
	CookieSessionConfig {
		cookie_name: "sessionid".to_string(),
		sliding_ttl: std::time::Duration::from_secs(1800),
		absolute_max: std::time::Duration::from_secs(86400),
		secure: !settings.core.debug,
		same_site: "Lax".to_string(),
		skip_paths: vec![
			"/api/auth/".to_string(),
			"/api/openapi.json".to_string(),
			"/api/docs".to_string(),
			"/api/redoc".to_string(),
		],
	}
}

// ── Router infrastructure ───────────────────────────────────────────

/// Pre-built infrastructure components for the application router.
///
/// Groups the DI context, admin routes with DI registrations,
/// session backend, and middleware configuration so they can be
/// resolved as a single dependency by `make_router`.
pub(crate) struct RouterInfrastructure {
	pub di_ctx: Arc<InjectionContext>,
	pub admin_router: ServerRouter,
	pub admin_di: DiRegistrationList,
	pub session_backend: Arc<RedisSessionBackend>,
	pub allowed_origins: AllowedOrigins,
	pub session_config: CookieSessionConfig,
}

/// DI factory — builds shared router infrastructure components.
///
/// Resolves `AllowedOrigins`, `CookieSessionConfig`, `WsBroadcaster`,
/// and `LocalAuthService` from the DI registry, then constructs the
/// DI context, admin site routes, and Redis session backend.
/// `WsBroadcaster` and `LocalAuthService` are injected solely to
/// trigger their singleton initialization.
#[injectable_factory(scope = "transient")]
async fn create_router_infrastructure(
	#[inject] allowed_origins: Depends<AllowedOrigins>,
	#[inject] session_config: Depends<CookieSessionConfig>,
	#[inject] _ws_broadcaster: Depends<WsBroadcaster>,
	#[inject] _local_auth_service: Depends<LocalAuthService>,
) -> RouterInfrastructure {
	let di_ctx = get_di_context(ContextLevel::Root);

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
		allowed_origins: (*allowed_origins).clone(),
		session_config: (*session_config).clone(),
	}
}

// ── Router construction ─────────────────────────────────────────────

/// Entry point for the `#[routes]` macro (called by the framework).
///
/// The `#[inject]` parameter resolves `UnifiedRouter` from the DI registry,
/// which triggers the `make_router` factory and all its transitive dependencies.
/// The framework creates the `InjectionContext` automatically for async routes.
#[routes]
pub async fn routes(#[inject] router: Depends<UnifiedRouter>) -> UnifiedRouter {
	router
		.try_unwrap()
		.expect("UnifiedRouter has multiple owners after resolve")
}

/// Build the application router by resolving dependencies from DI.
///
/// Infrastructure components (`RouterInfrastructure`) are resolved
/// transitively from the DI registry. Tests can override singletons
/// like `AllowedOrigins` by pre-registering in the `SingletonScope`
/// before calling this function.
#[injectable_factory(scope = "transient")]
async fn make_router(
	#[inject] infra: Depends<RouterInfrastructure>,
) -> UnifiedRouter {
	let infra = infra
		.try_unwrap()
		.unwrap_or_else(|_| panic!("RouterInfrastructure has multiple owners after resolve"));

	UnifiedRouter::new()
		// Admin panel
		.mount("/admin/", infra.admin_router)
		.mount("/static/admin/", admin_static_routes())
		.with_prefix("/api/")
		.with_di_registrations(infra.admin_di)
		// REST API endpoints
		.mount("/auth/", crate::apps::auth::urls::url_patterns())
		.mount("/clusters/", crate::apps::clusters::urls::url_patterns())
		.mount("/deployments/", crate::apps::deployments::urls::url_patterns())
		.server(|s| {
			s.server_fn(server::login::login::marker)
				.server_fn(server::register::register::marker)
				.server_fn(server::logout::logout::marker)
				.server_fn(server::me::me::marker)
		})
		.with_di_context(infra.di_ctx)
		.with_middleware(SecurityMiddleware::new())
		.with_middleware(CspPathMiddleware)
		.with_middleware(OriginGuardMiddleware::new(infra.allowed_origins.0))
		.with_middleware(CookieSessionAuthMiddleware::with_config(
			infra.session_backend,
			infra.session_config,
		))
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
