//! URL configuration for the health app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use crate::apps::health::server_urls;

/// Returns the unified URL patterns for the health app.
///
/// The health app currently exposes only a server-side liveness probe;
/// the empty `.client(|c| c)` block keeps the composition pattern
/// uniform across all apps.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(server_urls::healthz);
			s
		})
		.client(|c| c)
}
