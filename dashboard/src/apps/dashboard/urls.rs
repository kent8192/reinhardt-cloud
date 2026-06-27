//! URL configuration for the dashboard app.
//!
//! Declares the project-level SPA `home` route reachable from the
//! top-level navigation. The page component owns its route path and name
//! through `#[component(...)]`; this module only mounts that component into
//! the unified router.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::dashboard::client::layout::dashboard_shell;

/// Returns the unified URL patterns for the dashboard app.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| s)
		.client(|c| c.component(dashboard_shell))
}

#[cfg(all(test, native))]
mod tests {
	use reinhardt::urls::prelude::UnifiedRouter;
	use rstest::rstest;

	#[rstest]
	fn dashboard_home_route_is_registered_from_component_metadata() {
		// Arrange
		let router = UnifiedRouter::new()
			.mount_unified("/", super::url_patterns())
			.into_client();

		// Act
		let route = router.reverse("dashboard:home", &[]);

		// Assert
		assert_eq!(route, Ok("/".to_string()));
	}
}
