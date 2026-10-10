//! The `LoginLink` model.

use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::user::User;

/// A single-use, short-lived sign-in grant for one existing User (SR-16 to SR-18).
///
/// Only the SHA-256 digest of the link secret is stored, so a database read
/// yields no working sign-in. Rows are created by `manage create-login-link`
/// and consumed by the one conditional update in
/// `crate::apps::accounts::services::server::login_links`; there is no other
/// writer. The row outlives its use so the audit trail of a consumed or expired
/// link can still be explained; it is removed with its User.
#[model(
	app_label = "accounts",
	table_name = "accounts_login_links",
	constraints = [
		unique(fields = ["token_hash"], name = "accounts_login_links_token_hash_uniq")
	]
)]
#[derive(Default, Serialize, Deserialize)]
pub struct LoginLink {
	/// Opaque identifier, generated on insert.
	#[field(primary_key = true, include_in_new = false)]
	pub id: Uuid,

	/// The User the link signs in. A link never creates a User (SR-18).
	#[rel(foreign_key, related_name = "login_links", on_delete = Cascade)]
	pub user: ForeignKeyField<User>,

	/// Lowercase hex SHA-256 of the link secret. The secret itself is never stored.
	#[field(max_length = 64, skip_info = true)]
	pub token_hash: String,

	/// When the link stops working, fixed at issuance (SR-17).
	pub expires_at: DateTime<Utc>,

	/// When the link was used; none while it is still unused (SR-16).
	#[field(null = true)]
	pub consumed_at: Option<DateTime<Utc>>,

	/// Issuance timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}
