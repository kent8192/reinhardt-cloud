//! URL configuration for clusters app.

use reinhardt::ServerRouter;

use crate::apps::clusters::views;

/// Returns the URL patterns for cluster endpoints.
pub fn url_patterns() -> ServerRouter {
	ServerRouter::new()
		.endpoint(views::list_clusters)
		.endpoint(views::create_cluster)
		.endpoint(views::retrieve_cluster)
		.endpoint(views::update_cluster)
		.endpoint(views::delete_cluster)
}
