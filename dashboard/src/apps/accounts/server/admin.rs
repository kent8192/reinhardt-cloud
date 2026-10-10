//! Admin registrations of the accounts application.
//!
//! The admin site is reachable only by Staff: the framework's admin
//! authentication extractor rejects an inactive User and any User whose Staff
//! flag is not set before a `ModelAdmin` is consulted. The registrations below
//! narrow what Staff can do inside the site:
//!
//! - Users are never created or deleted here (they are created by sign-in or
//!   `manage grant-staff`), identity and Staff fields are read-only so the
//!   admin site cannot grant Staff (SR-20) or move a User to another GitHub
//!   account (SR-107), and only activation can be changed.
//! - Provider tokens are not shown at all, in any form, and the link row is
//!   read-only (SR-06, SR-102).
//! - Login Links are listed read-only without their digest: only
//!   `manage create-login-link` issues one (SR-18).

pub mod login_link;
pub mod social_account;
pub mod user;

use reinhardt::admin::core::AdminResult;
use reinhardt::admin::{AdminSite, AdminUser};

use crate::apps::accounts::models::User;

use self::login_link::LoginLinkAdmin;
use self::social_account::SocialAccountAdmin;
use self::user::UserAdmin;

/// Register every accounts model admin with `site`.
///
/// The project's admin-site assembly calls this once; keeping the list here
/// means a new accounts model cannot be forgotten at the project level.
///
/// # Errors
///
/// Returns an error when a model name or table is already registered.
pub fn register_model_admins(site: &AdminSite) -> AdminResult<()> {
	site.register("User", UserAdmin)?;
	site.register("Social Account", SocialAccountAdmin)?;
	site.register("Login Link", LoginLinkAdmin)
}

/// A `User` reaches the admin site only as active Staff; the framework checks
/// [`AdminUser::is_active`] and [`AdminUser::is_staff`] before any model admin
/// runs. There is no superuser tier: Staff is the only elevated role.
impl AdminUser for User {
	fn is_active(&self) -> bool {
		self.is_active
	}

	fn is_staff(&self) -> bool {
		self.is_staff
	}

	fn is_superuser(&self) -> bool {
		false
	}

	fn get_username(&self) -> &str {
		&self.github_login
	}
}
