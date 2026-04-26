//! Admin configuration for SocialAccount model.

use reinhardt::admin;

use crate::apps::auth::models::SocialAccount;

#[admin(model,
	for = SocialAccount,
	name = "Social Account",
	list_display = [id, user_id, provider, provider_username, created_at],
	list_filter = [provider],
	search_fields = [provider_user_id, provider_username],
	ordering = [(created_at, desc)],
	readonly_fields = [id, provider, provider_user_id, created_at, updated_at],
	list_per_page = 25,
	permissions = allow_all
)]
pub struct SocialAccountAdmin;
