//! URL configuration for the health app.

pub mod ws_urls;

use reinhardt::url_patterns;
use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use crate::apps::health::views;
use crate::config::apps::InstalledApp;

/// Returns the unified URL patterns for the health app.
///
/// The health app currently exposes only a server-side liveness probe;
/// the empty `.client(|c| c)` block keeps the composition pattern
/// uniform across all apps.
#[url_patterns(InstalledApp::health, mode = unified)]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(views::healthz);
			s
		})
		.client(|c| c)
}
