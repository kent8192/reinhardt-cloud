//! Admin configuration for the `LoginLink` model.
//!
//! Staff can see that a link was issued, for whom, and whether it was used, but
//! cannot create, edit, or delete one: Login Links are issued only by
//! `manage create-login-link` (SR-18). `fields` and `list_display` name no
//! `token_hash` column, so even the digest is not shown.

use reinhardt::admin;

use crate::apps::accounts::models::LoginLink;

#[admin(model,
	for = LoginLink,
	name = "Login Link",
	list_display = [id, user_id, expires_at, consumed_at, created_at],
	fields = [id, user_id, expires_at, consumed_at, created_at],
	readonly_fields = [id, user_id, expires_at, consumed_at, created_at],
	ordering = [(created_at, desc)],
	list_per_page = 25,
	allow_view = true
)]
pub struct LoginLinkAdmin;
