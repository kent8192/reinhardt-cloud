//! Admin configuration for User model.

use crate::apps::auth::models::User;
use reinhardt::admin;

#[admin(model,
	for = User,
	name = "User",
	list_display = [id, username, email, is_staff, is_active, date_joined],
	list_filter = [is_active, is_staff, is_superuser],
	search_fields = [username, email, first_name, last_name],
	ordering = [(date_joined, desc)],
	readonly_fields = [id, date_joined, updated_at],
	list_per_page = 25,
	permissions = allow_all,
)]
pub struct UserAdmin;
