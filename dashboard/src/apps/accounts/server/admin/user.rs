//! Admin configuration for the `User` model.
//!
//! The admin site shows Users and changes nothing about them. Every field is
//! read-only and changing a User is not permitted at all (reinhardt-admin
//! rejects any submitted read-only field on the server, and `allow_change =
//! false` refuses the update and inline-edit server functions before they look
//! at the data):
//!
//! - `is_staff` and the GitHub identity are changed only by `manage grant-staff`
//!   and `manage repoint-github-account` (SR-20, SR-107);
//! - `is_active` is changed only by `manage reactivate-user` (and deactivation
//!   happens through the fail-closed paths). Reactivating a User without first
//!   ending their sessions would bring back every session that nobody presented
//!   while the User was inactive, and reinhardt-admin 0.4.0-alpha.20 has no
//!   asynchronous save or transition hook to end them first: `AdminForm`
//!   (`normalize` and `validate`) is synchronous and sees neither the stored
//!   record nor the User's ID, and `ModelAdmin` has no save hook.

use reinhardt::admin;

use crate::apps::accounts::models::User;

#[admin(model,
	for = User,
	name = "User",
	list_display = [id, github_user_id, github_login, display_name, is_active, is_staff, created_at],
	list_filter = [is_active, is_staff],
	search_fields = [github_login, display_name],
	ordering = [(created_at, desc)],
	readonly_fields = [id, github_user_id, github_login, display_name, avatar_url, email, is_active, is_staff, last_login, created_at, updated_at],
	list_per_page = 25,
	allow_view = true
)]
pub struct UserAdmin;
