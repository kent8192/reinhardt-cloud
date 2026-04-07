//! URL configuration for Reinhardt Cloud.
//!
//! The `routes` function defines all URL patterns for this project,
//! including REST API endpoints and server function registrations
//! for the WASM frontend.
//!
//! ## DI-based configuration
//!
//! Singletons such as `AllowedOrigins`, `WsBroadcaster`, and
//! `LocalAuthService` are resolved from the `SingletonScope`.
//! Tests can pre-register overrides before calling `build_routes(scope)`.
//!
//! ## WebSocket routes
//!
//! WebSocket route registration uses `WebSocketRouter` from
//! reinhardt, which is async and independent of `UnifiedRouter`.
//! See `init_websocket_routes()` below.

use std::sync::Arc;

use reinhardt::admin::{admin_routes_with_di_deferred, core::admin_static_routes};
use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::pages::server_fn::ServerFnRouterExt;
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(not(target_arch = "wasm32"))]
use reinhardt::{WebSocketRoute, WebSocketRouter, register_websocket_router};

use crate::apps::auth::server;
use crate::apps::auth::services::LocalAuthService;
use crate::apps::realtime::WsBroadcaster;
use crate::config::middleware::CspPathMiddleware;
use reinhardt::{
	CookieSessionAuthMiddleware, CookieSessionConfig, OriginGuardMiddleware, RedisSessionBackend,
	SecurityMiddleware,
};

/// Allowed origins for the `OriginGuardMiddleware`.
///
/// Pre-register in `SingletonScope` to override the default origins
/// read from `[cors].allow_origins` in settings.
///
/// # Test override
///
/// ```ignore
/// let scope = Arc::new(SingletonScope::new());
/// scope.set(AllowedOrigins(vec![test_server_url]));
/// let router = build_routes(scope);
/// ```
pub struct AllowedOrigins(pub Vec<String>);

/// Resolve allowed origins from DI or settings.
///
/// Priority: DI override > `[cors].allow_origins` > default localhost.
fn resolve_origins(di_ctx: &InjectionContext) -> Vec<String> {
	if let Some(injected) = di_ctx.get_singleton::<AllowedOrigins>() {
		return injected.0.clone();
	}

	let settings = crate::config::settings::get_settings();
	let mut origins = settings.cors.allow_origins.clone();
	// Filter out wildcard "*" — OriginGuard uses exact matching
	origins.retain(|o| o != "*");
	if origins.is_empty() {
		let port = std::env::var("PORT").unwrap_or("8000".to_string());
		vec![
			format!("http://localhost:{port}"),
			format!("http://127.0.0.1:{port}"),
		]
	} else {
		origins
	}
}

/// Register default singletons unless already pre-registered (e.g. by tests).
fn register_defaults(di_ctx: &InjectionContext) {
	if di_ctx.get_singleton::<WsBroadcaster>().is_none() {
		di_ctx.set_singleton(WsBroadcaster::new());
	}
	if di_ctx.get_singleton::<LocalAuthService>().is_none() {
		di_ctx.set_singleton(LocalAuthService::new());
	}
}

/// Entry point for the `#[routes]` macro (called by the framework).
///
/// Delegates to [`build_routes`] with a fresh `SingletonScope`.
/// Tests should call `build_routes()` directly with a pre-configured scope.
#[routes]
pub fn routes() -> UnifiedRouter {
	build_routes(Arc::new(SingletonScope::new()))
}

/// Build the router with a pre-configured `SingletonScope`.
///
/// Singletons registered in the scope take precedence over defaults:
///
/// | Singleton | Default | Override in tests |
/// |-----------|---------|-------------------|
/// | `AllowedOrigins` | `[cors].allow_origins` | `scope.set(AllowedOrigins(vec![url]))` |
/// | `WsBroadcaster` | `WsBroadcaster::new()` | `scope.set(mock_broadcaster)` |
/// | `LocalAuthService` | `LocalAuthService::new()` | `scope.set(mock_auth)` |
pub fn build_routes(singleton_scope: Arc<SingletonScope>) -> UnifiedRouter {
	let di_ctx = Arc::new(InjectionContext::builder(Arc::clone(&singleton_scope)).build());

	let origins = resolve_origins(&di_ctx);
	register_defaults(&di_ctx);

	// Configure admin site with deferred DI registration.
	let admin_site = Arc::new(crate::config::admin::configure_admin());
	let (admin_router, admin_di) = admin_routes_with_di_deferred(admin_site);

	// Configure Redis-backed session authentication
	let redis_url = crate::config::settings::get_redis_url()
		.expect("Redis URL must be configured: set REINHARDT_CLOUD_REDIS_URL env var or redis_url in settings TOML");
	let session_backend = Arc::new(
		RedisSessionBackend::new_from_url(&redis_url)
			.expect("Failed to create Redis session backend"),
	);

	let session_config = CookieSessionConfig {
		cookie_name: "sessionid".to_string(),
		sliding_ttl: std::time::Duration::from_secs(1800),
		absolute_max: std::time::Duration::from_secs(86400),
		secure: !crate::config::settings::get_settings().core.debug,
		same_site: "Lax".to_string(),
		skip_paths: vec![
			"/api/auth/".to_string(),
			"/api/openapi.json".to_string(),
			"/api/docs".to_string(),
			"/api/redoc".to_string(),
		],
	};

	UnifiedRouter::new()
		// Admin panel
		.mount("/admin/", admin_router)
		.mount("/static/admin/", admin_static_routes())
		.with_di_registrations(admin_di)
		// REST API endpoints
		.mount("/api/", crate::apps::auth::urls::url_patterns())
		.mount("/api/", crate::apps::clusters::urls::url_patterns())
		.mount("/api/", crate::apps::deployments::urls::url_patterns())
		.server(|s| {
			s.server_fn(server::login::login::marker)
				.server_fn(server::register::register::marker)
				.server_fn(server::logout::logout::marker)
				.server_fn(server::me::me::marker)
		})
		.with_di_context(di_ctx)
		.with_middleware(SecurityMiddleware::new())
		.with_middleware(CspPathMiddleware)
		.with_middleware(OriginGuardMiddleware::new(origins))
		.with_middleware(CookieSessionAuthMiddleware::with_config(
			session_backend,
			session_config,
		))
}

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
