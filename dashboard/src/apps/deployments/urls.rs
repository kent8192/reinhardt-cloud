//! Client SPA routes for the deployments app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::deployments::client::pages::deployments_list_page;

/// Returns the unified URL patterns for the deployments app.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| s)
		.client(|c| c.page("list", "/deployments", deployments_list_page))
}
