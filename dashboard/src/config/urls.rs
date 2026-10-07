//! Compose App routes; domain endpoints remain inside their owning Apps.

use reinhardt::{UnifiedRouter, routes};

#[routes]
pub fn routes() -> UnifiedRouter {
	let router = UnifiedRouter::new()
		.merge(crate::apps::identity::urls::url_patterns())
		.merge(crate::apps::organization::urls::url_patterns())
		.merge(crate::apps::project::urls::url_patterns())
		.merge(crate::apps::source::urls::url_patterns())
		.merge(crate::apps::deployment::urls::url_patterns())
		.merge(crate::apps::cluster::urls::url_patterns())
		.merge(crate::apps::secret::urls::url_patterns())
		.merge(crate::apps::observability::urls::url_patterns());
	#[cfg(server)]
	let router = router.with_middleware(crate::apps::identity::middleware::IdentityMiddleware);
	router
}
