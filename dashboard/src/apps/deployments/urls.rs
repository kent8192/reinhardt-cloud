//! URL configuration for deployments app.

use reinhardt::ServerRouter;

use crate::apps::deployments::views;

/// Returns the URL patterns for deployment endpoints.
pub fn url_patterns() -> ServerRouter {
	ServerRouter::new()
		.endpoint(views::list_deployments)
		.endpoint(views::create_deployment)
}
