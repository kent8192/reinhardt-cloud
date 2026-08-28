//! URL configuration for the organizations app.
//!
//! Sub-issue #415 introduces the data layer only. URL endpoints
//! (`GET /api/orgs/`, `POST /api/orgs/`, etc.) are introduced by
//! sub-issue #418 as part of the broader URL reshape.

#[cfg(server)]
pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(server)]
use reinhardt::pages::router::ClientRouter;

#[cfg(server)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(server))]
type AppRouter = UnifiedRouter;

/// Returns the unified URL patterns for the organizations app.
///
/// No endpoints exist yet — the app's data layer landed in #415 but
/// HTTP endpoints will be introduced in #418. The empty `.server` and
/// `.client` blocks keep the file aligned with the per-app
/// `mount_unified` composition pattern.
#[cfg(server)]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new().client(|client| client)
}

#[cfg(not(server))]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
}
