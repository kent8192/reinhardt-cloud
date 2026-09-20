//! Client SPA routes for the deployments app.

#[cfg(server)]
pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(server)]
use crate::apps::deployments::server_urls;

/// Returns the unified URL patterns for the deployments app.
#[cfg(server)]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new().server(|server| server.endpoint(server_urls::cli_deploy))
}

#[cfg(not(server))]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
}

#[cfg(all(test, server))]
mod tests {
	use reinhardt::urls::prelude::UnifiedRouter;
	use rstest::rstest;

	#[rstest]
	fn cli_deploy_route_is_registered_under_api_prefix() {
		// Arrange
		let router = UnifiedRouter::new()
			.with_prefix("/api/")
			.mount_unified("/", super::url_patterns())
			.into_server();

		// Act
		let url = router.reverse("cli-deploy", &[]);

		// Assert
		assert_eq!(url, Some("/api/deployments/cli/".to_string()));
	}
}
