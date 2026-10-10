//! The admin site, restricted to Staff.
//!
//! Every model of every application is registered here and reachable only by
//! active Staff: the framework's admin authentication extractor loads the
//! `User` from the database on each request and refuses it unless it is active
//! and Staff (SR-07, SR-20), whatever the session once said.

use std::sync::Arc;

use reinhardt::admin::AdminSite;
use reinhardt::admin::core::AdminResult;

use crate::apps::accounts::models::User;
use crate::apps::accounts::server::admin::register_model_admins;

/// Build the admin site.
///
/// `set_user_type::<User>()` is what makes the admin loader read the `User`
/// model (and re-check `is_active` and `is_staff`) instead of the framework's
/// default user table.
///
/// # Errors
///
/// Returns an error when a model admin cannot be registered.
pub fn build_admin_site() -> AdminResult<Arc<AdminSite>> {
	let mut site = AdminSite::new("Reinhardt Cloud");
	site.set_user_type::<User>();
	register_model_admins(&site)?;
	Ok(Arc::new(site))
}
