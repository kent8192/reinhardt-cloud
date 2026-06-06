//! Client SPA routes for the clusters app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::clusters::client::pages::clusters_list_page;

/// Returns the unified URL patterns for the clusters app.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| s)
		.client(|c| c.route("list", "/clusters", clusters_list_page))
}
