//! URL configuration for GitHub App integration.

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(server)]
use reinhardt::pages::router::ClientRouter;

#[cfg(server)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(server))]
type AppRouter = UnifiedRouter;

#[cfg(server)]
use crate::apps::github::server_urls;

#[cfg(server)]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
		.server(|server| {
			server
				.endpoint(server_urls::github_setup)
				.endpoint(server_urls::github_webhook)
		})
		.client(|client| client)
}

#[cfg(not(server))]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
}

#[cfg(all(test, server))]
mod tests {
	use reinhardt::urls::prelude::UnifiedRouter;
	use rstest::rstest;

	#[rstest]
	fn github_webhook_route_is_registered_under_api_prefix() {
		// Arrange
		let router = UnifiedRouter::new()
			.with_prefix("/api/")
			.mount_unified("/github/", super::url_patterns())
			.into_server();

		// Act
		let url = router.reverse("github-webhook", &[]);

		// Assert
		assert_eq!(url, Some("/api/github/webhooks/github/".to_string()));
	}

	#[rstest]
	fn github_setup_route_is_registered_under_api_prefix() {
		// Arrange
		let router = UnifiedRouter::new()
			.with_prefix("/api/")
			.mount_unified("/github/", super::url_patterns())
			.into_server();

		// Act
		let url = router.reverse("github-setup", &[]);

		// Assert
		assert_eq!(url, Some("/api/github/setup/".to_string()));
	}
}
