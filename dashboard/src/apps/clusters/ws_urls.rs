//! WebSocket URL configuration for clusters app.
//!
//! Currently empty — clusters has no per-handler WebSocket endpoints.
//! The module is required by the `#[routes]` macro to generate the
//! `urls.ws().clusters()` accessor and its underlying `ws_url_resolvers`
//! sub-module via `#[url_patterns(mode = ws)]`.

use reinhardt::WebSocketRouter;
use reinhardt::url_patterns;

use crate::config::apps::InstalledApp;

/// Returns the WebSocket URL patterns for cluster endpoints (none today).
#[url_patterns(InstalledApp::clusters, mode = ws)]
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new()
}
