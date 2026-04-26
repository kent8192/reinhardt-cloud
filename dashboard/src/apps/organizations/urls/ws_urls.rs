//! WebSocket URL configuration for the organizations app.
//!
//! Currently empty -- the organizations app has no WebSocket endpoints.
//! The module is required by the `#[routes]` macro to generate the
//! `urls.ws().organizations()` accessor and its underlying
//! `ws_url_resolvers` sub-module via `#[url_patterns(mode = ws)]`.

use reinhardt::WebSocketRouter;
use reinhardt::url_patterns;

use crate::config::apps::InstalledApp;

/// Returns the WebSocket URL patterns for organizations endpoints (none today).
#[url_patterns(InstalledApp::organizations, mode = ws)]
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new()
}
