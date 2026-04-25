//! Admin configuration for Cluster model.

use reinhardt::admin;

use crate::apps::clusters::models::Cluster;

#[admin(model,
	for = Cluster,
	name = "Cluster",
	list_display = [id, organization_id, name, api_url, is_active, created_at],
	list_filter = [is_active],
	search_fields = [name, api_url],
	ordering = [(created_at, desc)],
	readonly_fields = [id, created_at, updated_at],
	list_per_page = 25,
	permissions = allow_all
)]
pub struct ClusterAdmin;
