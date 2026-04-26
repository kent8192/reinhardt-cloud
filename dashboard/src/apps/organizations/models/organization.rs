//! Organization ORM model.
//!
//! Multi-tenant boundary: every owned resource (Cluster, Deployment, …)
//! belongs to exactly one Organization. The `slug` doubles as the K8s
//! namespace name once sub-issue #416 lands.

use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

/// Multi-tenant Organization, owns Clusters and Deployments.
#[derive(Serialize, Deserialize)]
#[model(app_label = "organizations", table_name = "organizations")]
pub struct Organization {
	/// Primary key (None for auto-increment on insert).
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// URL slug, also used as the K8s namespace name.
	/// Must conform to DNS-1123 label (`^[a-z]([-a-z0-9]*[a-z0-9])?$`),
	/// max 63 characters, globally unique.
	#[field(max_length = 63, unique = true)]
	pub slug: String,

	/// Display name shown in dashboard UI.
	#[field(max_length = 100)]
	pub name: String,

	/// Organization creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: chrono::DateTime<chrono::Utc>,

	/// Last update timestamp.
	#[field(auto_now = true)]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}
