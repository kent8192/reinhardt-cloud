//! URL configuration for auth app.

pub mod ws_urls;

use reinhardt::ServerRouter;
use reinhardt::url_patterns;

use crate::apps::auth::views;
use crate::config::apps::InstalledApp;

/// Returns the URL patterns for auth endpoints.
#[url_patterns(InstalledApp::auth, mode = server)]
pub fn server_url_patterns() -> ServerRouter {
	ServerRouter::new()
		.endpoint(views::login)
		.endpoint(views::register)
		.endpoint(views::verify_email)
		.endpoint(views::forgot_password)
		.endpoint(views::reset_password)
		.endpoint(views::profile)
		.endpoint(views::profile_update)
		.endpoint(views::change_password)
		.endpoint(views::verify_email_change)
}
