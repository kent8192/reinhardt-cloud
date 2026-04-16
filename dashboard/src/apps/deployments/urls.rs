//! URL configuration for deployments app.

use reinhardt::ServerRouter;
use reinhardt::url_patterns;

use crate::apps::deployments::views;

/// Returns the URL patterns for deployment endpoints.
#[url_patterns(app = "deployments")]
pub fn url_patterns() -> ServerRouter {
	ServerRouter::new()
		.endpoint(views::list_deployments)
		.endpoint(views::create_deployment)
		.endpoint(views::retrieve_deployment)
		.endpoint(views::update_deployment)
		.endpoint(views::delete_deployment)
		.endpoint(views::deployment_logs)
		.endpoint(views::deployment_status)
}
