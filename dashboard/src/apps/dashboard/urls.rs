//! URL configuration for the dashboard app.
//!
//! Declares the project-level SPA `home` route reachable from the
//! top-level navigation. Server-side reverse URL resolution uses
//! `UrlReverser::from_global()` and SPA navigation uses hardcoded paths.
//! Per-section routes (`clusters:list`, `deployments:list`) are owned by
//! their respective apps' `url_patterns`.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::dashboard::client::layout::dashboard_shell;

/// Returns the unified URL patterns for the dashboard app.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| s)
		.client(|c| c.route("home", "/", dashboard_shell))
}
