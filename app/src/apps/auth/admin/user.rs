//! Admin configuration for User model.

use reinhardt::admin;

use crate::apps::auth::models::User;

#[admin(model,
	for = User,
	name = "User",
	list_display = [id, username, email, is_active, is_staff, is_superuser, last_login, date_joined],
	list_filter = [is_active, is_staff, is_superuser],
	search_fields = [username, email],
	ordering = [(date_joined, desc)],
	readonly_fields = [id, password_hash, last_login, date_joined, updated_at],
	list_per_page = 25
)]
pub struct UserAdmin;
