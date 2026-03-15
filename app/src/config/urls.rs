//! URL configuration for nuages project (RESTful)
//!
//! The `routes` function defines all URL patterns for this project.

// Workaround for kent8192/reinhardt-web#2362: UnifiedRouter is not re-exported via
// prelude when client-router feature is disabled. Import explicitly.
// Remove this workaround when the upstream issue is resolved.
use reinhardt::urls::prelude::UnifiedRouter;
use reinhardt::routes;

#[routes]
pub fn routes() -> UnifiedRouter {
	UnifiedRouter::new()
		.mount("/api/", crate::apps::auth::urls::url_patterns())
		.mount("/api/", crate::apps::clusters::urls::url_patterns())
		.mount("/api/", crate::apps::deployments::urls::url_patterns())
}
