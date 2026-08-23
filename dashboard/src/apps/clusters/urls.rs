//! Client SPA routes for the clusters app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use reinhardt::pages::router::ClientRouter;

#[cfg(native)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(native))]
type AppRouter = UnifiedRouter;

use crate::apps::clusters::client::pages::clusters_list_page;

/// Returns the unified URL patterns for the clusters app.
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
		.server(|s| s)
		.client(|c| c.component(clusters_list_page))
}

#[cfg(all(test, native))]
mod tests {
	use reinhardt::urls::prelude::UnifiedRouter;
	use rstest::rstest;

	#[rstest]
	fn clusters_page_route_is_registered_from_component_metadata() {
		// Arrange
		let router = UnifiedRouter::new()
			.mount_unified("/", super::url_patterns())
			.into_client();

		// Act
		let route = router.reverse("clusters:list", &[]);

		// Assert
		assert_eq!(route, Ok("/clusters".to_string()));
	}
}
