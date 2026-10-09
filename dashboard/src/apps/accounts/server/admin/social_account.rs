//! Admin configuration for the `SocialAccount` model.
//!
//! `fields` and `list_display` name only non-secret columns, so the encrypted
//! token envelopes can be neither listed nor edited from the admin site.

use reinhardt::admin;

use crate::apps::accounts::models::SocialAccount;

#[admin(model,
	for = SocialAccount,
	name = "Social Account",
	list_display = [id, user_id, access_token_expires_at, refresh_token_expires_at, created_at],
	fields = [id, user_id, access_token_expires_at, refresh_token_expires_at, created_at, updated_at],
	readonly_fields = [id, user_id, access_token_expires_at, refresh_token_expires_at, created_at, updated_at],
	ordering = [(created_at, desc)],
	list_per_page = 25,
	allow_view = true
)]
pub struct SocialAccountAdmin;
