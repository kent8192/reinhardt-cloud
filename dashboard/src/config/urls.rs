//! URL configuration for the Control Plane.
//!
//! `routes` is the single project-level registration. Each application exposes
//! one `url_patterns()` aggregate for HTTP, WebSocket, gRPC, and client routes;
//! merge them explicitly below, one `merge` per installed application. On the
//! server the merged routes are then wrapped with the shared request surface
//! (admin site, middleware, dependency injection) by `config::web`.
//!
//! `routes` must stay synchronous: `#[routes]` registers the browser's client
//! routes only for a synchronous function, and an `async` one would leave the
//! single-page application without any route.

use reinhardt::UnifiedRouter;
use reinhardt::routes;

#[routes]
pub fn routes() -> UnifiedRouter {
	// One merge per installed app. `url_patterns()` is target-neutral; no
	// server/client cfg branch is needed here.
	let router = UnifiedRouter::new()
		.merge(crate::apps::accounts::urls::url_patterns())
		.merge(crate::apps::organizations::urls::url_patterns())
		.merge(crate::apps::clusters::urls::url_patterns())
		.merge(crate::apps::agents::urls::url_patterns())
		.merge(crate::apps::projects::urls::url_patterns())
		.merge(crate::apps::deployments::urls::url_patterns())
		.merge(crate::apps::logs::urls::url_patterns())
		.merge(crate::apps::github::urls::url_patterns())
		.merge(crate::apps::health::urls::url_patterns());

	#[cfg(server)]
	let router = crate::config::web::assemble(router);
	router
}
