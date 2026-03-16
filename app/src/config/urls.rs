//! URL configuration for nuages project (RESTful)
//!
//! The `routes` function defines all URL patterns for this project.

use reinhardt::urls::prelude::UnifiedRouter;
use reinhardt::routes;

#[routes]
pub fn routes() -> UnifiedRouter {
	UnifiedRouter::new()
		.mount("/api/", crate::apps::auth::urls::url_patterns())
		.mount("/api/", crate::apps::clusters::urls::url_patterns())
		.mount("/api/", crate::apps::deployments::urls::url_patterns())
}
