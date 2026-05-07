//! URL configuration for the deployments app.

pub mod ws_urls;

use reinhardt::url_patterns;
use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use crate::apps::deployments::views;
use crate::config::apps::InstalledApp;

/// Returns the unified URL patterns for the deployments app.
///
/// Server endpoints and (currently absent) client SPA routes are merged
/// into a single function via `mode = unified`. The empty
/// `.client(|c| c)` block keeps the composition pattern uniform across
/// all apps.
#[url_patterns(InstalledApp::deployments, mode = unified)]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(views::list_deployments)
				.endpoint(views::create_deployment)
				.endpoint(views::retrieve_deployment)
				.endpoint(views::update_deployment)
				.endpoint(views::delete_deployment)
				.endpoint(views::deployment_logs)
				.endpoint(views::deployment_status);
			s
		})
		.client(|c| c)
}
