//! Client SPA routes for the clusters app.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use reinhardt::pages::router::ClientRouter;

#[cfg(native)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(native))]
type AppRouter = UnifiedRouter;

/// Returns the unified URL patterns for the clusters app.
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new().server(|s| s).client(|c| c)
}
