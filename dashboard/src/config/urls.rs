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
//! `#[injectable_factory]`. The router builder resolves them from the
//! DI context at startup.
//!
//! ## WebSocket routes
//!
//! WebSocket route registration uses `WebSocketRouter` from
//! reinhardt, which is async and independent of `UnifiedRouter`.
//! See `init_websocket_routes()` below.

use std::sync::Arc;

use reinhardt::admin::{admin_routes_with_di, admin_static_routes};
use reinhardt::di::{ContextLevel, Depends, get_di_context, injectable_factory};
use reinhardt::pages::server_fn::ServerFnRouterExt;
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

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

// ── Router construction ─────────────────────────────────────────────

/// Entry point for the `#[routes]` macro (called by the framework).
///
/// The `#[inject]` parameter resolves `UnifiedRouter` from the DI registry,
/// which triggers the `make_router` factory and all its transitive dependencies.
/// The framework creates the `InjectionContext` automatically for async routes.
#[routes]
pub async fn routes(#[inject] router: Arc<UnifiedRouter>) -> UnifiedRouter {
	Arc::try_unwrap(router).expect("UnifiedRouter has multiple owners after resolve")
}

/// Build the application router by resolving dependencies from DI.
///
/// All singletons (`AllowedOrigins`, `CookieSessionConfig`,
/// `WsBroadcaster`, `LocalAuthService`) are resolved from the
/// DI registry. Tests can override any of them by pre-registering
/// in the `SingletonScope` before calling this function.
#[injectable_factory(scope = "transient")]
async fn make_router(
	#[inject] allowed_origins: Depends<AllowedOrigins>,
	#[inject] session_config: Depends<CookieSessionConfig>,
	#[inject] _ws_broadcaster: Arc<WsBroadcaster>,
	#[inject] _local_auth_service: Depends<LocalAuthService>,
) -> UnifiedRouter {
	let di_ctx = get_di_context(ContextLevel::Root);

	// Configure admin site with DI registration.
	let admin_site = Arc::new(crate::config::admin::configure_admin());
	let (admin_router, admin_di) = admin_routes_with_di(admin_site);

	// Configure Redis-backed session backend
	let redis_url = crate::config::settings::get_redis_url()
		.expect("Redis URL must be configured: set REINHARDT_CLOUD_REDIS_URL env var or redis_url in settings TOML");
	let session_backend = Arc::new(
		RedisSessionBackend::new_from_url(&redis_url)
			.expect("Failed to create Redis session backend"),
	);

	UnifiedRouter::new()
		// Admin panel
		.mount("/admin/", admin_router)
		.mount("/static/admin/", admin_static_routes())
		.with_prefix("/api/")
		.with_di_registrations(admin_di)
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
		.with_di_context(di_ctx)
		.with_middleware(SecurityMiddleware::new())
		.with_middleware(CspPathMiddleware)
		.with_middleware(OriginGuardMiddleware::new(allowed_origins.0.clone()))
		.with_middleware(CookieSessionAuthMiddleware::with_config(
			session_backend,
			(*session_config).clone(),
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
