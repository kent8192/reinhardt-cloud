//! URL configuration for the organizations app.
//!
//! Sub-issue #415 introduces the data layer only. URL endpoints
//! (`GET /api/orgs/`, `POST /api/orgs/`, etc.) are introduced by
//! sub-issue #418 as part of the broader URL reshape.
//!
//! This file exists to satisfy the framework's `#[routes]` macro, which
//! expects every installed app to expose a `urls` module with both
//! server- and websocket-side patterns.

pub mod ws_urls;

use reinhardt::ServerRouter;
use reinhardt::url_patterns;

use crate::config::apps::InstalledApp;

/// Returns an empty URL pattern set for the organizations app.
///
/// Endpoints will be added by sub-issue #418 (URL reshape).
#[url_patterns(InstalledApp::organizations, mode = server)]
pub fn server_url_patterns() -> ServerRouter {
	ServerRouter::new()
}
