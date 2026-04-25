//! WebSocket URL configuration for health app.
//!
//! Currently empty — the health app has no WebSocket endpoints. The
//! module is required by the `#[routes]` macro to generate the
//! `urls.ws().health()` accessor and its underlying `ws_url_resolvers`
//! sub-module via `#[url_patterns(mode = ws)]`.

use reinhardt::WebSocketRouter;
use reinhardt::url_patterns;

use crate::config::apps::InstalledApp;

/// Returns the WebSocket URL patterns for health endpoints (none today).
#[url_patterns(InstalledApp::health, mode = ws)]
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new()
}
