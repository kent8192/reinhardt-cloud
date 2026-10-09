//! URL configuration for cloud_control_plane project (Pages).
//!
//! The routes function is the single project-level registration. Each
//! application exposes one url_patterns() aggregate for HTTP, WebSocket, gRPC,
//! and client routes; merge those values explicitly below.
//!
//! Module application example:
//!     let router = router.merge(crate::apps::chat::urls::url_patterns());
//!     let router = router.merge(crate::apps::accounts::urls::url_patterns());
//!
//! Workspace application example:
//!     let router = router.merge(chat::urls::url_patterns());
//!     let router = router.merge(accounts::urls::url_patterns());

use reinhardt::prelude::*;
use reinhardt::routes;

#[routes]
pub fn routes() -> UnifiedRouter {
	// One merge per installed app. `url_patterns()` is target-neutral; no
	// server/client cfg branch is needed here.
	UnifiedRouter::new()
		.merge(crate::apps::accounts::urls::url_patterns())
		.merge(crate::apps::organizations::urls::url_patterns())
		.merge(crate::apps::clusters::urls::url_patterns())
		.merge(crate::apps::agents::urls::url_patterns())
		.merge(crate::apps::projects::urls::url_patterns())
		.merge(crate::apps::deployments::urls::url_patterns())
		.merge(crate::apps::logs::urls::url_patterns())
		.merge(crate::apps::github::urls::url_patterns())
		.merge(crate::apps::health::urls::url_patterns())
}
