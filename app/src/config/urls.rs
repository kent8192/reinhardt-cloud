//! URL configuration for nuages project.
//!
//! The `routes` function defines all URL patterns for this project,
//! including REST API endpoints and server function registrations
//! for the WASM frontend.
//!
//! WebSocket route registration uses `WebSocketRouter` from
//! reinhardt-websockets, which is async and independent of `UnifiedRouter`.
//! See `init_websocket_routes()` below.

use std::sync::Arc;

use reinhardt::admin::admin_routes;
use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::pages::server_fn::ServerFnRouterExt;
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(not(target_arch = "wasm32"))]
use reinhardt::{WebSocketRoute, WebSocketRouter, register_websocket_router};

use crate::apps::auth::server;
use crate::apps::realtime::WsBroadcaster;
use crate::config::middleware::{JwtAuthMiddleware, SecurityHeadersMiddleware};

#[routes]
pub fn routes() -> UnifiedRouter {
	let singleton_scope = Arc::new(SingletonScope::new());
	let di_ctx = Arc::new(InjectionContext::builder(singleton_scope).build());

	// Register the WebSocket broadcaster as a singleton so that other
	// services (e.g. deployment status updaters) can obtain it via DI
	// and push events to connected clients.
	// NOTE: Do not wrap in Arc — set_singleton() wraps internally,
	// and double-wrapping causes TypeId mismatch during DI resolution.
	di_ctx.set_singleton(WsBroadcaster::new());

	UnifiedRouter::new()
		// Admin panel
		.mount("/admin/", admin_routes())
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
		.with_middleware(JwtAuthMiddleware)
		.with_middleware(SecurityHeadersMiddleware)
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
