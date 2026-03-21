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

// WebSocket route registration:
//
// The reinhardt-websockets `WebSocketRouter` / `WebSocketRoute` types are
// not re-exported from the `reinhardt` facade crate. Once they are
// available (or reinhardt-websockets is added as a direct dependency),
// register the `/ws/notifications` endpoint as follows:
//
//   use reinhardt_websockets::routing::{
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
//
// The `WsBroadcaster` is already registered as a DI singleton above, so
// it can be resolved by any service that needs to push events to
// connected WebSocket clients.
