//! Model surface for GitHub App installation metadata.

use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

use crate::apps::organizations::models::Organization;

/// GitHub App installation bound to an organization.
#[model(app_label = "github", table_name = "github_installations")]
#[derive(Serialize, Deserialize)]
pub struct GitHubInstallation {
	/// Primary key (None for auto-increment on insert).
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Organization that owns this GitHub App installation.
	#[rel(foreign_key, related_name = "github_installations")]
	pub organization: ForeignKeyField<Organization>,

	/// GitHub installation identifier.
	#[field(unique = true)]
	pub installation_id: i64,

	/// GitHub account identifier for the installation target.
	pub account_id: i64,

	/// GitHub account login for display and reconciliation.
	#[field(max_length = 255)]
	pub account_login: String,

	/// GitHub account type, such as User or Organization.
	#[field(max_length = 32)]
	pub account_type: String,

	/// Installation lifecycle status.
	#[field(max_length = 32)]
	pub status: String,

	/// Installation creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,

	/// Last update timestamp.
	#[field(auto_now = true)]
	pub updated_at: DateTime<Utc>,
}
