//! URL configuration for the health app.

#[cfg(server)]
pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(server)]
use crate::apps::health::server_urls;

/// Returns the unified URL patterns for the health app.
///
/// The health app exposes only a server-side liveness probe.
#[cfg(server)]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new().server(|server| server.endpoint(server_urls::healthz))
}

#[cfg(not(server))]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
}
