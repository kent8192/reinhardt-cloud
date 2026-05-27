//! URL configuration for the clusters app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::clusters::client::pages::clusters_list_page;
#[cfg(native)]
use crate::apps::clusters::views;

/// Returns the unified URL patterns for the clusters app.
///
/// Server endpoints and the SPA `clusters:list` route are merged into a
/// single `UnifiedRouter`. The named route resolves to
/// `clusters_list_page` (a placeholder delegating to the shared 404 view
/// until a dedicated list page lands), and `mount_unified` in
/// `config/urls.rs` aggregates both sides into the project router.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(views::list_clusters)
				.endpoint(views::create_cluster)
				.endpoint(views::retrieve_cluster)
				.endpoint(views::update_cluster)
				.endpoint(views::delete_cluster)
				.endpoint(views::rotate_token);
			s
		})
		.client(|c| c.route("list", "/clusters", clusters_list_page))
}
