//! Admin configuration for the `User` model.

use reinhardt::admin;

use crate::apps::accounts::models::User;

#[admin(model,
	for = User,
	name = "User",
	list_display = [id, github_user_id, github_login, display_name, is_active, is_staff, created_at],
	list_filter = [is_active, is_staff],
	search_fields = [github_login, display_name],
	ordering = [(created_at, desc)],
	readonly_fields = [id, github_user_id, github_login, display_name, avatar_url, email, is_staff, created_at, updated_at],
	list_per_page = 25,
	allow_view = true,
	allow_change = true
)]
pub struct UserAdmin;
