//! URL configuration for GitHub App integration.

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use crate::apps::github::server_urls;

pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new().server(|s| {
		#[cfg(native)]
		let s = s.endpoint(server_urls::github_webhook);
		s
	})
}

#[cfg(all(test, native))]
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
}
