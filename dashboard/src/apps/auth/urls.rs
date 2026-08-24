//! URL configuration for the auth app.
//!
//! Client SPA routes for the auth app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use reinhardt::pages::router::ClientRouter;

#[cfg(native)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(native))]
type AppRouter = UnifiedRouter;

#[cfg(native)]
use crate::apps::auth::server_urls;

/// Returns the unified URL patterns for the auth app.
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(server_urls::verify_email)
				.endpoint(server_urls::oauth_start)
				.endpoint(server_urls::oauth_callback)
				.endpoint(server_urls::api_me);
			s
		})
		.client(|c| c)
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
}
