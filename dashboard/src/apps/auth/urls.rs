//! URL configuration for the auth app.
//!
//! Client SPA routes for the auth app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::auth::client::pages::{login_page, register_page};
#[cfg(native)]
use crate::apps::auth::server_urls;

/// Returns the unified URL patterns for the auth app.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(server_urls::verify_email);
			s
		})
		.client(|c| {
			c.route("login_page", "/login", login_page).route(
				"register_page",
				"/register",
				register_page,
			)
		})
}
