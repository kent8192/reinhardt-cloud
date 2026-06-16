//! Admin configuration for Deployment model.

use reinhardt::admin;

use crate::apps::deployments::models::Deployment;

#[admin(model,
	for = Deployment,
	name = "Deployment",
	list_display = [id, organization_id, project_name, cluster_id, status, image, created_at],
	list_filter = [status],
	search_fields = [project_name, image],
	ordering = [(created_at, desc)],
	readonly_fields = [id, created_at, updated_at],
	list_per_page = 25,
	permissions = allow_all
)]
pub struct DeploymentAdmin;
