//! Admin configuration for User model.

use reinhardt::admin;

use crate::apps::auth::models::User;

#[admin(model,
	for = User,
	name = "User",
	list_display = [id, username, email, is_active, last_login, created_at],
	list_filter = [is_active],
	search_fields = [username, email],
	ordering = [(created_at, desc)],
	readonly_fields = [id, password_hash, last_login, created_at, updated_at],
	list_per_page = 25
)]
pub struct UserAdmin;
