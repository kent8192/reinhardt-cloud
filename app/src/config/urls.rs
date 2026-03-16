//! URL configuration for nuages project (RESTful)
//!
//! The `routes` function defines all URL patterns for this project.

use std::sync::Arc;

use reinhardt::db::orm::get_connection;
use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

use crate::config::middleware::JwtAuthMiddleware;

#[routes]
pub fn routes() -> UnifiedRouter {
	let singleton_scope = Arc::new(SingletonScope::new());

	// Register DatabaseConnection in DI so CurrentUser<User> can resolve
	// authenticated users from the database.
	let db = get_connection();
	let di_ctx = Arc::new(
		InjectionContext::builder(singleton_scope)
			.singleton(db)
			.build(),
	);

	UnifiedRouter::new()
		.with_di_context(di_ctx)
		.mount("/api/", crate::apps::auth::urls::url_patterns())
		.mount("/api/", crate::apps::clusters::urls::url_patterns())
		.mount("/api/", crate::apps::deployments::urls::url_patterns())
		.with_middleware(JwtAuthMiddleware)
}
