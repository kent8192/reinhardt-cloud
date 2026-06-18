//! URL configuration for the auth app.
//!
//! Client SPA routes for the auth app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::auth::client::pages::{account_page, login_page, register_page};
#[cfg(native)]
use crate::apps::auth::server_urls;

/// Returns the unified URL patterns for the auth app.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(server_urls::verify_email)
				.endpoint(server_urls::oauth_start)
				.endpoint(server_urls::oauth_callback);
			s
		})
		.client(|c| {
			c.page("account_page", "/account", account_page)
				.page("login_page", "/login", login_page)
				.page("register_page", "/register", register_page)
		})
}

#[cfg(all(test, native))]
mod tests {
	use reinhardt::urls::prelude::UnifiedRouter;
	use rstest::rstest;

	#[rstest]
	fn oauth_routes_are_registered_under_auth_api_prefix() {
		// Arrange
		let router = UnifiedRouter::new()
			.with_prefix("/api/")
			.mount_unified("/auth/", super::url_patterns())
			.into_server();

		// Act
		let start = router.reverse("oauth-start", &[("provider_id", "github")]);
		let callback = router.reverse("oauth-callback", &[("provider_id", "github")]);

		// Assert
		assert_eq!(start, Some("/api/auth/oauth/github/start/".to_string()));
		assert_eq!(
			callback,
			Some("/api/auth/oauth/github/callback/".to_string())
		);
	}

	#[rstest]
	fn account_page_route_is_registered() {
		// Arrange
		let router = UnifiedRouter::new()
			.mount_unified("/", super::url_patterns())
			.into_client();

		// Act
		let account = router.reverse("account_page", &[]);

		// Assert
		assert_eq!(account, Ok("/account".to_string()));
	}
}
