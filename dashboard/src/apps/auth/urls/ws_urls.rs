//! WebSocket URL configuration for auth app.
//!
//! Currently empty — auth has no per-handler WebSocket endpoints.
//! The module is required by the `#[routes]` macro to generate the
//! `urls.ws().auth()` accessor and its underlying `ws_url_resolvers`
//! sub-module via `#[url_patterns(mode = ws)]`.

use reinhardt::WebSocketRouter;
use reinhardt::url_patterns;

use crate::config::apps::InstalledApp;

/// Returns the WebSocket URL patterns for auth endpoints (none today).
#[url_patterns(InstalledApp::auth, mode = ws)]
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new()
}
