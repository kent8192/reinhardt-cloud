//! Social account model.
//!
//! Links a local `User` to a third-party identity provider (GitHub, GitLab,
//! ...). One row per (user, provider) link; the same user may link several
//! providers. The pair `(provider, provider_user_id)` is globally unique so
//! that the same external identity cannot be claimed by two local users.
//!
//! No long-term token is persisted: the OAuth access token is exchanged in
//! memory during callback, used to fetch user info, then dropped. Refresh
//! tokens are out of scope until a future "deploy from your repository"
//! feature explicitly opts in.

use chrono::{DateTime, Utc};
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Link between a local `User` and an external OAuth/OIDC provider account.
///
/// The `id` is a UUID rather than an auto-increment integer to match the
/// `reinhardt-auth` `SocialAccountStorage` trait surface (its `delete`
/// takes `Uuid`), and to keep enumeration of links non-trivial.
#[derive(Default, Serialize, Deserialize)]
#[model(app_label = "auth", table_name = "auth_social_accounts")]
pub struct SocialAccount {
	/// Primary key (UUID v4, generated on insert).
	#[field(primary_key = true, include_in_new = false)]
	pub id: Uuid,

	/// Owning user (foreign key to `auth_users.id`).
	pub user_id: Uuid,

	/// Provider identifier — `"github"`, `"gitlab"`, etc. Lowercase, stable.
	#[field(max_length = 32)]
	pub provider: String,

	/// Stable identifier from the provider (numeric for GitHub/GitLab,
	/// stored as a string so heterogeneous provider id formats fit).
	#[field(max_length = 255)]
	pub provider_user_id: String,

	/// Display name from the provider (login, preferred_username). Optional
	/// because some providers may not surface it.
	#[field(max_length = 255, null = true)]
	pub provider_username: Option<String>,

	/// Creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,

	/// Last-update timestamp.
	#[field(auto_now = true)]
	pub updated_at: DateTime<Utc>,
}
