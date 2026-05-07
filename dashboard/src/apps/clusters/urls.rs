//! URL configuration for the clusters app.

pub mod ws_urls;

use reinhardt::url_patterns;
use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use crate::apps::clusters::views;
use crate::config::apps::InstalledApp;

/// Returns the unified URL patterns for the clusters app.
///
/// Server endpoints and (currently absent) client SPA routes are merged
/// into a single function via `mode = unified`. The empty
/// `.client(|c| c)` block keeps the composition pattern uniform across
/// all apps so `config/urls.rs` can call `mount_unified` once per app.
#[url_patterns(InstalledApp::clusters, mode = unified)]
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
		.client(|c| c)
}
