//! Model surface for repositories visible through GitHub App installations.

use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

use super::GitHubInstallation;

/// GitHub repository visible through a GitHub App installation.
#[model(app_label = "github", table_name = "github_repositories")]
#[derive(Serialize, Deserialize)]
pub struct GitHubRepository {
	/// Primary key (None for auto-increment on insert).
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Installation that grants repository access.
	#[rel(foreign_key, related_name = "repositories")]
	pub installation: ForeignKeyField<GitHubInstallation>,

	/// GitHub repository identifier.
	#[field(unique = true)]
	pub github_repository_id: i64,

	/// Repository full name in `owner/name` form.
	#[field(max_length = 512)]
	pub full_name: String,

	/// Repository owner login.
	#[field(max_length = 255)]
	pub owner_login: String,

	/// Repository short name.
	#[field(max_length = 255)]
	pub name: String,

	/// Repository default branch.
	#[field(max_length = 255)]
	pub default_branch: String,

	/// Whether the repository is private.
	pub private: bool,

	/// Whether the repository is selected for Reinhardt Cloud use.
	pub selected: bool,

	/// Repository record creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,

	/// Last update timestamp.
	#[field(auto_now = true)]
	pub updated_at: DateTime<Utc>,
}
