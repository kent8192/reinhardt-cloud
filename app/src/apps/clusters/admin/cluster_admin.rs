//! Admin configuration for Cluster model.

use crate::apps::clusters::models::Cluster;
use reinhardt::admin;

#[admin(model,
	for = Cluster,
	name = "Cluster",
	list_display = [id, user_id, name, api_url, is_active, created_at],
	fields = [user_id, name, api_url, is_active],
	list_filter = [is_active],
	search_fields = [name, api_url],
	ordering = [(created_at, desc)],
	readonly_fields = [id, created_at, updated_at],
	list_per_page = 25,
	permissions = allow_all,
)]
pub struct ClusterAdmin;
