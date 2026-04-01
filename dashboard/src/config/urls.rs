//! URL configuration for Reinhardt Cloud project (RESTful)
//!
//! The `routes` function defines all URL patterns for this project.

use std::sync::Arc;

use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

use crate::config::middleware::{
	DiRequestMiddleware, JwtAuthMiddleware, SecurityHeadersMiddleware,
};

#[routes]
pub fn routes() -> UnifiedRouter {
	let singleton_scope = Arc::new(SingletonScope::new());
	let di_ctx = Arc::new(InjectionContext::builder(singleton_scope).build());

	UnifiedRouter::new()
		.mount("/api/", crate::apps::auth::urls::url_patterns())
		.mount("/api/", crate::apps::clusters::urls::url_patterns())
		.mount("/api/", crate::apps::deployments::urls::url_patterns())
		.with_di_context(di_ctx)
		.with_middleware(JwtAuthMiddleware)
		.with_middleware(DiRequestMiddleware)
		.with_middleware(SecurityHeadersMiddleware)
}
