//! Client SPA routes for the clusters app.

#[cfg(server)]
pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(server)]
use reinhardt::pages::router::ClientRouter;

#[cfg(server)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(server))]
type AppRouter = UnifiedRouter;

/// Returns the unified URL patterns for the clusters app.
#[cfg(server)]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new().client(|client| client)
}

#[cfg(not(server))]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
}
