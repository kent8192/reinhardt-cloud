//! GitHub repository import project model.

use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

use crate::apps::deployments::models::Deployment;
use crate::apps::github::models::GitHubRepository;
use crate::apps::organizations::models::Organization;

/// A Reinhardt Cloud project imported from one GitHub repository.
#[model(app_label = "github", table_name = "github_projects")]
#[derive(Serialize, Deserialize)]
pub struct GitHubProject {
	/// Primary key (None for auto-increment on insert).
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Organization that owns the imported project.
	#[rel(foreign_key, related_name = "github_projects")]
	pub organization: ForeignKeyField<Organization>,

	/// Imported GitHub repository. Unique at the database layer.
	#[rel(foreign_key, related_name = "github_project")]
	pub repository: ForeignKeyField<GitHubRepository>,

	/// Production deployment generated for this repository.
	#[rel(foreign_key, related_name = "github_project")]
	pub deployment: ForeignKeyField<Deployment>,

	/// Reinhardt project name generated for the repository.
	#[field(max_length = 63)]
	pub project_name: String,

	/// Production branch tracked by this project.
	#[field(max_length = 255)]
	pub production_branch: String,

	/// Project import lifecycle status.
	#[field(max_length = 32)]
	pub status: String,

	/// Project creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,

	/// Last update timestamp.
	#[field(auto_now = true)]
	pub updated_at: DateTime<Utc>,
}
