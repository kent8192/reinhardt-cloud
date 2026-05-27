//! WebSocket URL configuration for the deployments app.
//!
//! Currently empty — deployments has no per-handler WebSocket endpoints.
//! The module is required by the `#[routes]` macro to generate the
//! `urls.ws().deployments()` accessor and its underlying `ws_url_resolvers`
//! sub-module.

use reinhardt::WebSocketRouter;

/// Returns the WebSocket URL patterns for deployment endpoints (none today).
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new()
}
