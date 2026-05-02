//! WebSocket URL configuration for the dashboard app.
//!
//! Currently empty — the dashboard app exposes only client-side SPA
//! routes. The module is required by the `#[routes]` macro to generate
//! the `urls.ws().dashboard()` accessor and its underlying
//! `ws_url_resolvers` sub-module via `#[url_patterns(mode = ws)]`.

use reinhardt::WebSocketRouter;
use reinhardt::url_patterns;

use crate::config::apps::InstalledApp;

/// Returns the WebSocket URL patterns for dashboard endpoints (none today).
#[url_patterns(InstalledApp::dashboard, mode = ws)]
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new()
}
