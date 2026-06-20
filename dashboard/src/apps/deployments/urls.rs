//! Client SPA routes for the deployments app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::deployments::client::pages::deployments_list_page;
#[cfg(native)]
use crate::apps::deployments::server_urls;

/// Returns the unified URL patterns for the deployments app.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(server_urls::cli_deploy);
			s
		})
		.client(|c| c.page("list", "/deployments", deployments_list_page))
}

#[cfg(all(test, native))]
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
