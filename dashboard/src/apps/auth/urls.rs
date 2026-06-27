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
				.endpoint(server_urls::oauth_callback)
				.endpoint(server_urls::api_me);
			s
		})
		.client(|c| {
			c.component(account_page)
				.component(login_page)
				.component(register_page)
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
	fn api_me_route_is_registered_under_auth_api_prefix() {
		// Arrange
		let router = UnifiedRouter::new()
			.with_prefix("/api/")
			.mount_unified("/auth/", super::url_patterns())
			.into_server();

		// Act
		let me = router.reverse("api-me", &[]);

		// Assert — the CLI calls this endpoint to verify a bearer token.
		assert_eq!(me, Some("/api/auth/me/".to_string()));
	}

	#[rstest]
	fn account_page_route_is_registered() {
		// Arrange
		let router = UnifiedRouter::new()
			.mount_unified("/", super::url_patterns())
			.into_client();

		// Act
		let account = router.reverse("auth:account_page", &[]);

		// Assert
		assert_eq!(account, Ok("/account".to_string()));
	}

	#[rstest]
	fn login_and_register_page_routes_are_registered_from_component_metadata() {
		// Arrange
		let router = UnifiedRouter::new()
			.mount_unified("/", super::url_patterns())
			.into_client();

		// Act
		let login = router.reverse("auth:login_page", &[]);
		let register = router.reverse("auth:register_page", &[]);

		// Assert
		assert_eq!(login, Ok("/login".to_string()));
		assert_eq!(register, Ok("/register".to_string()));
	}
}
