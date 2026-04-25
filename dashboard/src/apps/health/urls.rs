//! URL configuration for health app.

pub mod ws_urls;

use reinhardt::ServerRouter;
use reinhardt::url_patterns;

use crate::apps::health::views;
use crate::config::apps::InstalledApp;

/// Returns the URL patterns for health endpoints.
#[url_patterns(InstalledApp::health, mode = server)]
pub fn server_url_patterns() -> ServerRouter {
	ServerRouter::new().endpoint(views::healthz)
}
