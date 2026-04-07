//! URL configuration for auth app.

use reinhardt::ServerRouter;

use crate::apps::auth::views;

/// Returns the URL patterns for auth endpoints.
pub fn url_patterns() -> ServerRouter {
	ServerRouter::new()
		.endpoint(views::login)
		.endpoint(views::register)
		.endpoint(views::profile)
		.endpoint(views::profile_update)
		.endpoint(views::change_password)
}
