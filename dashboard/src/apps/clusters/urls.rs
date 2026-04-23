//! URL configuration for clusters app.

use reinhardt::ServerRouter;
use reinhardt::url_patterns;

use crate::apps::clusters::views;
use crate::config::apps::InstalledApp;

/// Returns the URL patterns for cluster endpoints.
#[url_patterns(InstalledApp::clusters, mode = server)]
pub fn server_url_patterns() -> ServerRouter {
	ServerRouter::new()
		.endpoint(views::list_clusters)
		.endpoint(views::create_cluster)
		.endpoint(views::retrieve_cluster)
		.endpoint(views::update_cluster)
		.endpoint(views::delete_cluster)
		.endpoint(views::rotate_token)
}
