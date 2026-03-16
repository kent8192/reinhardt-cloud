//! URL configuration for nuages project (RESTful)
//!
//! The `routes` function defines all URL patterns for this project.

use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

use crate::config::middleware::JwtAuthMiddleware;

#[routes]
pub fn routes() -> UnifiedRouter {
	UnifiedRouter::new()
		.mount("/api/", crate::apps::auth::urls::url_patterns())
		.mount("/api/", crate::apps::clusters::urls::url_patterns())
		.mount("/api/", crate::apps::deployments::urls::url_patterns())
		.with_middleware(JwtAuthMiddleware)
}
