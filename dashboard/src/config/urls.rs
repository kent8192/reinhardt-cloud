//! URL configuration for Reinhardt Cloud.
//!
//! The `routes` function defines all URL patterns for this project,
//! including REST API endpoints and server function registrations
//! for the WASM frontend.
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
/// Register in `SingletonScope` before calling `routes()` to override
/// the default origins read from `[cors].allow_origins` in settings.
///
/// # Default behaviour
///
/// When not pre-registered, `routes()` reads `CorsSettings.allow_origins`
/// from `ProjectSettings`, filters out `"*"` (OriginGuard uses exact
/// matching), and falls back to `http://localhost:{PORT}` /
/// `http://127.0.0.1:{PORT}` when the list is empty.
///
/// # Test override
///
/// ```ignore
/// let scope = Arc::new(SingletonScope::new());
/// scope.set(AllowedOrigins(vec![test_server_url]));
/// let router = routes(scope);
/// ```
pub struct AllowedOrigins(pub Vec<String>);

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
/// If `AllowedOrigins` is registered in the scope, it is used directly
/// for the `OriginGuardMiddleware`.  Otherwise, origins are read from
/// `[cors].allow_origins` in the project settings.
///
/// # Test override
///
/// ```ignore
/// let scope = Arc::new(SingletonScope::new());
/// scope.set(AllowedOrigins(vec![test_server_url]));
/// let router = build_routes(scope).into_server();
/// ```
pub fn build_routes(singleton_scope: Arc<SingletonScope>) -> UnifiedRouter {
	let di_ctx = Arc::new(InjectionContext::builder(Arc::clone(&singleton_scope)).build());

	// Resolve AllowedOrigins: DI override > settings > default localhost
	let origins = if let Some(injected) = di_ctx.get_singleton::<AllowedOrigins>() {
		injected.0.clone()
	} else {
		let settings = crate::config::settings::get_settings();
		let mut from_settings = settings.cors.allow_origins.clone();
		// Filter out wildcard "*" — OriginGuard uses exact matching
		from_settings.retain(|o| o != "*");
		if from_settings.is_empty() {
			let port = std::env::var("PORT").unwrap_or("8000".to_string());
			vec![
				format!("http://localhost:{port}"),
				format!("http://127.0.0.1:{port}"),
			]
		} else {
			from_settings
		}
	};

	// Configure admin site with deferred DI registration.
	// AdminSite is captured in DiRegistrationList and applied to the server's
	// singleton scope during startup. AdminDatabase is lazily constructed from
	// DatabaseConnection at first request.
	let admin_site = Arc::new(crate::config::admin::configure_admin());
	let (admin_router, admin_di) = admin_routes_with_di_deferred(admin_site);

	// Register the WebSocket broadcaster as a singleton so that other
	// services (e.g. deployment status updaters) can obtain it via DI
	// and push events to connected clients.
	// NOTE: Do not wrap in Arc — set_singleton() wraps internally,
	// and double-wrapping causes TypeId mismatch during DI resolution.
	di_ctx.set_singleton(WsBroadcaster::new());

	// Register AuthService for trait-based authentication across REST and gRPC.
	// NOTE: Register the concrete type directly — set_singleton() wraps in Arc
	// internally, so passing Arc<dyn AuthService> would create Arc<Arc<...>>.
	di_ctx.set_singleton(LocalAuthService::new());

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
