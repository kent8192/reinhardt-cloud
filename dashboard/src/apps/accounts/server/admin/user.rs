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
//! - `is_active` is changed only by `manage deactivate-user` and
//!   `manage reactivate-user` (and by the fail-closed paths). Reactivating a
//!   User without first ending their sessions would bring back every session
//!   that nobody presented while the User was inactive; see the workaround note
//!   on [`UserAdmin`].

use reinhardt::admin;

use crate::apps::accounts::models::User;

// Workaround for kent8192/reinhardt-web#6725 (tracked in
// kent8192/reinhardt-cloud#956): reinhardt-admin 0.4.0-alpha.20 has no
// asynchronous save or transition hook (`AdminForm::normalize` and `validate`
// are synchronous and see neither the stored record nor the primary key, and
// `ModelAdmin` has no save hook), so the sessions of a User cannot be ended
// before an admin-side reactivation. Every field of this admin is therefore
// read-only and changing a User is not permitted.
// Remove this workaround when the upstream issue is resolved.
//
// Ideal implementation (without workaround):
//   // `is_active` editable (not in `readonly_fields`), `allow_change` on, and
//   // a hook that ends the sessions on a false -> true transition, rejecting
//   // the save when that fails:
//   async fn before_save(&self, ctx: &AdminSaveContext<'_>, change: &AdminChange)
//       -> AdminResult<()> {
//       if change.transition::<bool>("is_active") == Some((false, true)) {
//           sessions.destroy_all_for_user(ctx.pk()).await.map_err(|_| {
//               AdminError::validation(
//                   "is_active",
//                   "the sessions could not be ended; use `manage reactivate-user`",
//               )
//           })?;
//           // and emit `accounts.reactivate.succeeded` like the command does
//       }
//       Ok(())
//   }
//   // Deactivating through the admin stays allowed: per-request revalidation
//   // already refuses the User's sessions.
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
