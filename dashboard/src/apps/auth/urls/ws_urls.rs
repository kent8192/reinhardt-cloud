//! WebSocket URL configuration for the auth app.
//!
//! Currently empty — auth has no per-handler WebSocket endpoints.
//! The module is required by the `#[routes]` macro to generate the
//! `urls.ws().auth()` accessor and its underlying `ws_url_resolvers`
//! sub-module.

use reinhardt::WebSocketRouter;

/// Returns the WebSocket URL patterns for auth endpoints (none today).
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new()
}
