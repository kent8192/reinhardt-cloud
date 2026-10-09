//! The `User` model.

use chrono::{DateTime, Utc};
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A person known to the Control Plane, identified by their GitHub account.
///
/// The numeric GitHub user ID is the only identity key (SR-02). The login, the
/// display name, the avatar, and the email are profile data copied from GitHub
/// at sign-in; none of them is ever used to find, merge, or authorize a User.
/// `github_user_id` is immutable: the only operation that moves a User to
/// another GitHub account is the host-operator re-pointing command (SR-107).
///
/// `id` is an internal opaque identifier (UUID) used by sessions and foreign
/// keys; it carries no meaning outside the Control Plane.
#[model(
	app_label = "accounts",
	table_name = "accounts_users",
	constraints = [
		unique(fields = ["github_user_id"], name = "accounts_users_github_user_id_uniq")
	]
)]
#[derive(Default, Serialize, Deserialize)]
pub struct User {
	/// Opaque internal identifier, generated on insert.
	#[field(primary_key = true, include_in_new = false)]
	pub id: Uuid,

	/// Numeric GitHub user ID. The identity key of the User (SR-02, SR-03).
	// nosemgrep: reinhardt-no-scalar-fk-id -- Immutable external GitHub identifier, not a relationship.
	pub github_user_id: i64,

	/// Current GitHub login. Display data only; it may change and is not unique.
	#[field(max_length = 64)]
	pub github_login: String,

	/// Name shown in the Dashboard; falls back to the login when GitHub has none.
	#[field(max_length = 255)]
	pub display_name: String,

	/// GitHub avatar URL, when GitHub reports one.
	#[field(max_length = 2048, null = true)]
	pub avatar_url: Option<String>,

	/// Verified primary email address, when one is known. Never a lookup key.
	#[field(max_length = 254, null = true)]
	pub email: Option<String>,

	/// Whether the User may sign in. Deactivation is an explicit operator action.
	#[field(default = true)]
	pub is_active: bool,

	/// Whether the User is Staff. Changed only by `manage grant-staff` (SR-20).
	#[field(default = false)]
	pub is_staff: bool,

	/// Creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,

	/// Last-update timestamp.
	#[field(auto_now = true)]
	pub updated_at: DateTime<Utc>,
}
