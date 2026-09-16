//! URL configuration for the organizations app.
//!
//! Sub-issue #415 introduces the data layer only. URL endpoints
//! (`GET /api/orgs/`, `POST /api/orgs/`, etc.) are introduced by
//! sub-issue #418 as part of the broader URL reshape.

#[cfg(server)]
pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

/// Returns the unified URL patterns for the organizations app.
///
/// The app participates in `mount_unified` composition without HTTP endpoints.
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
}
