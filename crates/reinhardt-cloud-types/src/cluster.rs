//! Cluster domain type representing a Kubernetes cluster managed by nuages.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A Kubernetes cluster registered with the nuages PaaS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
	pub id: Uuid,
	pub name: String,
	pub api_url: String,
	pub is_active: bool,
}

impl Cluster {
	/// Creates a new active cluster with a generated UUID.
	pub fn new(name: &str, api_url: &str) -> Self {
		Self {
			id: Uuid::new_v4(),
			name: name.to_string(),
			api_url: api_url.to_string(),
			is_active: true,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_cluster_new_sets_fields() {
		// Arrange
		let name = "prod-cluster";
		let api_url = "https://k8s.example.com:6443";

		// Act
		let cluster = Cluster::new(name, api_url);

		// Assert
		assert_eq!(cluster.name, name);
		assert_eq!(cluster.api_url, api_url);
		assert!(cluster.is_active);
	}

	#[rstest]
	fn test_cluster_new_generates_unique_ids() {
		// Arrange / Act
		let c1 = Cluster::new("a", "https://a.example.com");
		let c2 = Cluster::new("b", "https://b.example.com");

		// Assert
		assert_ne!(c1.id, c2.id);
	}

	#[rstest]
	fn test_cluster_serialization_roundtrip() {
		// Arrange
		let cluster = Cluster::new("test", "https://test.example.com");

		// Act
		let json = serde_json::to_string(&cluster).unwrap();
		let deserialized: Cluster = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.id, cluster.id);
		assert_eq!(deserialized.name, cluster.name);
		assert_eq!(deserialized.api_url, cluster.api_url);
		assert_eq!(deserialized.is_active, cluster.is_active);
	}
}
