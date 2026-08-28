//! Cross-target generated form support for cluster creation.
//!
//! The native target aliases the persistence model's generated form types.
//! WASM only needs the public creation payload, so it intentionally defines a
//! lightweight form model instead of exposing the ORM relationship graph.

use reinhardt::pages::form::ModelFormPolicy;

/// Public fields accepted when creating a cluster.
pub struct ClusterCreateFields;

impl ModelFormPolicy for ClusterCreateFields {
	fn allows(field: &str) -> bool {
		matches!(field, "name" | "api_url")
	}
}

#[cfg(native)]
pub use crate::apps::clusters::models::Cluster as ClusterCreateForm;
#[cfg(native)]
pub use crate::apps::clusters::models::{
	ClusterFormSchema as ClusterCreateFormFormSchema,
	ClusterModelFormData as ClusterCreateFormModelFormData,
};
#[cfg(native)]
pub use ClusterCreateFormFormSchema as ClusterCreateFormSchema;
#[cfg(native)]
pub use ClusterCreateFormModelFormData as ClusterCreateFormData;

#[cfg(wasm)]
use reinhardt::prelude::*;
#[cfg(wasm)]
use serde::{Deserialize, Serialize};

/// WASM-only model metadata for the public cluster creation payload.
#[cfg(wasm)]
#[model(
	app_label = "clusters",
	table_name = "clusters",
	form = true,
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct ClusterCreateForm {
	/// Macro metadata requires a primary key, but creation payloads never expose it.
	#[field(primary_key = true, editable = false)]
	pub id: Option<i64>,

	#[field(min_length = 1, max_length = 63)]
	pub name: String,

	#[field(url = true, max_length = 2048)]
	pub api_url: String,
}

#[cfg(wasm)]
pub use ClusterCreateFormFormSchema as ClusterCreateFormSchema;
#[cfg(wasm)]
pub use ClusterCreateFormModelFormData as ClusterCreateFormData;
