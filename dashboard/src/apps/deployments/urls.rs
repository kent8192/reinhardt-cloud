//! URL configuration for the deployments app.

pub mod ws_urls;

use reinhardt::url_patterns;
use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::deployments::client::pages::deployments_list_page;
#[cfg(native)]
use crate::apps::deployments::views;
use crate::config::apps::InstalledApp;

/// Returns the unified URL patterns for the deployments app.
///
/// Server endpoints and the SPA `deployments:list` route are merged into
/// a single function via `mode = unified`. The named route resolves to
/// `deployments_list_page` (a placeholder delegating to the shared 404
/// view until a dedicated list page lands), and `mount_unified` in
/// `config/urls.rs` aggregates both sides into the project router.
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
		.client(|c| c.named_route("list", "/deployments", deployments_list_page))
}
