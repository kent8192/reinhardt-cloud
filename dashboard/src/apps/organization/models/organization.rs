//! Organization persistence for the organization App.

use chrono::{DateTime, Utc};
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "organization",
	table_name = "cloud_organization_organization",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Organization {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[field(max_length = 200)]
	pub display_name: String,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}
