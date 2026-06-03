//! URL configuration for the organizations app.
//!
//! Sub-issue #415 introduces the data layer only. URL endpoints
//! (`GET /api/orgs/`, `POST /api/orgs/`, etc.) are introduced by
//! sub-issue #418 as part of the broader URL reshape.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

/// Returns the unified URL patterns for the organizations app.
///
/// No endpoints exist yet — the app's data layer landed in #415 but
/// HTTP endpoints will be introduced in #418. The empty `.server` and
/// `.client` blocks keep the file aligned with the per-app
/// `mount_unified` composition pattern.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new().server(|s| s).client(|c| c)
}
