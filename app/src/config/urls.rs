//! URL configuration for nuages project.
//!
//! The `routes` function defines all URL patterns for this project,
//! including REST API endpoints and server function registrations
//! for the WASM frontend.

use std::sync::Arc;

use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::pages::server_fn::ServerFnRouterExt;
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::auth::server;
use crate::config::middleware::{JwtAuthMiddleware, SecurityHeadersMiddleware};

#[routes]
pub fn routes() -> UnifiedRouter {
	let singleton_scope = Arc::new(SingletonScope::new());
	let di_ctx = Arc::new(InjectionContext::builder(singleton_scope).build());

	UnifiedRouter::new()
		// REST API endpoints
		.mount("/api/", crate::apps::auth::urls::url_patterns())
		.mount("/api/", crate::apps::clusters::urls::url_patterns())
		.mount("/api/", crate::apps::deployments::urls::url_patterns())
		.server(|s| {
			s.server_fn(server::login::login::marker)
				.server_fn(server::register::register::marker)
				.server_fn(server::logout::logout::marker)
				.server_fn(server::me::me::marker)
		})
		.with_di_context(di_ctx)
		.with_middleware(JwtAuthMiddleware)
		.with_middleware(SecurityHeadersMiddleware)
}
