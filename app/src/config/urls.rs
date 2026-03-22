//! URL configuration for nuages project.
//!
//! The `routes` function defines all URL patterns for this project,
//! including REST API endpoints and server function registrations
//! for the WASM frontend.
//!
//! WebSocket route registration requires the `WebSocketRouter` from
//! reinhardt-websockets, which is async and independent of `UnifiedRouter`.
//! See the inline comment at the end of this file for the planned approach.

use std::sync::Arc;

use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::pages::server_fn::ServerFnRouterExt;
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

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
	let broadcaster = Arc::new(WsBroadcaster::new());
	di_ctx.set_singleton(broadcaster);

	UnifiedRouter::new()
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

// Workaround for kent8192/reinhardt-web#2790 (tracked in reinhardt-cloud#108)
// Remove this workaround when the upstream issue is resolved.
//
// WebSocket route registration is skipped because the `WebSocketRouter`,
// `WebSocketRoute`, and `register_websocket_router` types are not
// re-exported from the `reinhardt` facade crate.
// The `WsBroadcaster` is registered as a DI singleton above, but the
// `/ws/notifications` endpoint is non-functional until this is resolved.
//
// Ideal implementation (without workaround):
//   use reinhardt::websockets::routing::{
//       WebSocketRoute, WebSocketRouter, register_websocket_router,
//   };
//
//   pub async fn init_websocket_routes() {
//       let mut ws_router = WebSocketRouter::new();
//       let route = WebSocketRoute::new(
//           "/ws/notifications".to_string(),
//           Some("websocket:notifications".to_string()),
//       );
//       ws_router.register_route(route).await
//           .expect("failed to register /ws/notifications route");
//       register_websocket_router(ws_router).await;
//   }
