//! Client SPA routes for the clusters app.

#[cfg(server)]
pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

/// Returns the unified URL patterns for the clusters app.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
}
