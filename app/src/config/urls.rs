//! URL configuration for Reinhardt Cloud project (RESTful)
//!
//! The `routes` function defines all URL patterns for this project.

use std::sync::Arc;

use reinhardt::admin::{AdminSite, admin_routes_with_di_deferred, admin_static_routes};
use reinhardt::di::{InjectionContext, SingletonScope};
use reinhardt::routes;
use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::auth::admin::UserAdmin;
use crate::apps::clusters::admin::ClusterAdmin;
use crate::apps::deployments::admin::DeploymentAdmin;
use crate::config::middleware::{JwtAuthMiddleware, SecurityHeadersMiddleware};

/// Configure the admin site with all model registrations.
fn configure_admin() -> AdminSite {
	let site = AdminSite::new("Reinhardt Cloud Admin");

	site.register("User", UserAdmin)
		.expect("Failed to register UserAdmin");
	site.register("Cluster", ClusterAdmin)
		.expect("Failed to register ClusterAdmin");
	site.register("Deployment", DeploymentAdmin)
		.expect("Failed to register DeploymentAdmin");

	site
}

#[routes]
pub fn routes() -> UnifiedRouter {
	let singleton_scope = Arc::new(SingletonScope::new());
	let di_ctx = Arc::new(InjectionContext::builder(singleton_scope).build());

	let admin_site = Arc::new(configure_admin());
	let (admin_router, admin_di) = admin_routes_with_di_deferred(admin_site);

	UnifiedRouter::new()
		.mount("/api/", crate::apps::auth::urls::url_patterns())
		.mount("/api/", crate::apps::clusters::urls::url_patterns())
		.mount("/api/", crate::apps::deployments::urls::url_patterns())
		.mount("/admin/", admin_router)
		.with_di_registrations(admin_di)
		.mount("/static/admin/", admin_static_routes())
		.with_di_context(di_ctx)
		.with_middleware(JwtAuthMiddleware)
		.with_middleware(SecurityHeadersMiddleware)
}
