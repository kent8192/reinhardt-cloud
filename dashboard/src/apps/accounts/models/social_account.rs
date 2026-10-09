//! The `SocialAccount` model.

use chrono::{DateTime, Utc};
use reinhardt::db::associations::OneToOneField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::user::User;

/// The GitHub tokens of one User, encrypted at rest (SR-06).
///
/// A row exists only after the User has completed a GitHub sign-in: a User
/// pre-provisioned by `manage grant-staff` has none. The GitHub identity itself
/// (the numeric user ID) lives only on `User`, so the two can never disagree
/// and the one-to-one relation enforces "a User has exactly one GitHub
/// identity" (SR-03) in the database.
///
/// The token columns hold envelopes produced by
/// `crate::apps::accounts::services::server::token_crypto`; they are never
/// readable as plaintext and are excluded from the generated info DTO and from
/// the admin site.
#[model(app_label = "accounts", table_name = "accounts_social_accounts")]
#[derive(Default, Serialize, Deserialize)]
pub struct SocialAccount {
	/// Primary key, generated on insert.
	#[field(primary_key = true, include_in_new = false)]
	pub id: Uuid,

	/// The User that owns these tokens.
	#[rel(one_to_one, related_name = "social_account", on_delete = Cascade)]
	pub user: OneToOneField<User>,

	/// Encrypted GitHub access token envelope.
	#[field(max_length = 8192, skip_info = true)]
	pub encrypted_access_token: String,

	/// Encrypted GitHub refresh token envelope, when GitHub issued one.
	#[field(max_length = 8192, skip_info = true, null = true)]
	pub encrypted_refresh_token: Option<String>,

	/// When the access token expires.
	pub access_token_expires_at: DateTime<Utc>,

	/// When the refresh token expires, when GitHub reports it.
	#[field(null = true)]
	pub refresh_token_expires_at: Option<DateTime<Utc>>,

	/// Creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,

	/// Last-update timestamp.
	#[field(auto_now = true)]
	pub updated_at: DateTime<Utc>,
}
