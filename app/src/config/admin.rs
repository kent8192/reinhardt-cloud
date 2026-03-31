//! Admin panel configuration for Reinhardt Cloud.

use reinhardt::admin::AdminSite;

use crate::apps::auth::admin::UserAdmin;
use crate::apps::auth::models::user::User;
use crate::apps::clusters::admin::ClusterAdmin;
use crate::apps::deployments::admin::DeploymentAdmin;
use crate::config::settings::get_jwt_secret;

/// Configure the admin site with all registered model admins.
pub fn configure_admin() -> AdminSite {
	let mut admin_site = AdminSite::new("Reinhardt Cloud Administration");

	// Configure authentication for admin login
	admin_site.set_user_type::<User>();
	if let Some(secret) = get_jwt_secret() {
		admin_site.set_jwt_secret(secret.as_bytes());
	}

	admin_site
		.register("User", UserAdmin)
		.expect("failed to register User admin");
	admin_site
		.register("Cluster", ClusterAdmin)
		.expect("failed to register Cluster admin");
	admin_site
		.register("Deployment", DeploymentAdmin)
		.expect("failed to register Deployment admin");
	admin_site
}
