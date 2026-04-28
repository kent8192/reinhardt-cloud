//! Unit tests for Cluster model construction and field behaviour.

#[cfg(test)]
mod tests {
	use reinhardt::db::orm::Model;
	use reinhardt::db::orm::inspection::ConstraintType;
	use rstest::rstest;

	use crate::apps::clusters::models::Cluster;

	#[rstest]
	fn test_cluster_new_sets_fields() {
		// Arrange
		let organization_id: i64 = 42;
		let name = "production".to_string();
		let api_url = "https://k8s.example.com:6443".to_string();

		// Act
		let cluster = Cluster::new(
			organization_id,
			name.clone(),
			api_url.clone(),
			true,
			None,
			None,
		);

		// Assert
		assert_eq!(cluster.organization_id, organization_id);
		assert_eq!(cluster.name, name);
		assert_eq!(cluster.api_url, api_url);
		assert!(cluster.is_active);
	}

	#[rstest]
	fn test_cluster_new_id_is_none() {
		// Arrange
		let organization_id: i64 = 7;

		// Act
		let cluster = Cluster::new(
			organization_id,
			"test-cluster".to_string(),
			"https://k8s.example.com:6443".to_string(),
			true,
			None,
			None,
		);

		// Assert
		assert_eq!(cluster.id, None);
	}

	#[rstest]
	#[case(true)]
	#[case(false)]
	fn test_cluster_is_active_flag(#[case] active: bool) {
		// Arrange
		let organization_id: i64 = 1;

		// Act
		let cluster = Cluster::new(
			organization_id,
			"flag-test".to_string(),
			"https://k8s.example.com:6443".to_string(),
			active,
			None,
			None,
		);

		// Assert
		assert_eq!(cluster.is_active, active);
	}

	/// The `unique_together = ("organization_id", "name")` declaration in
	/// `Cluster` MUST surface as a composite UNIQUE constraint via the
	/// model's `constraint_metadata()` (refs #436). This guards against
	/// silent regressions where the model attribute is removed or
	/// reordered.
	#[rstest]
	fn test_cluster_exposes_organization_name_unique_constraint() {
		// Arrange
		let constraints = Cluster::constraint_metadata();

		// Act
		let unique_constraint = constraints
			.iter()
			.find(|c| c.constraint_type == ConstraintType::Unique);

		// Assert
		let constraint = unique_constraint
			.expect("Cluster must expose a composite UNIQUE constraint for unique_together");
		assert_eq!(constraint.name, "clusters_organization_id_name_uniq");
		assert_eq!(
			constraint.definition, "UNIQUE (organization_id, name)",
			"Constraint definition must cover (organization_id, name) in that order"
		);
	}

	#[rstest]
	fn test_cluster_serialization_roundtrip() {
		// Arrange
		let cluster = Cluster::new(
			99,
			"roundtrip".to_string(),
			"https://k8s.example.com:6443".to_string(),
			true,
			None,
			None,
		);

		// Act
		let json = serde_json::to_string(&cluster).expect("serialize should succeed");
		let deserialized: Cluster =
			serde_json::from_str(&json).expect("deserialize should succeed");

		// Assert
		assert_eq!(deserialized.name, cluster.name);
		assert_eq!(deserialized.api_url, cluster.api_url);
		assert_eq!(deserialized.organization_id, cluster.organization_id);
		assert_eq!(deserialized.is_active, cluster.is_active);
		assert_eq!(deserialized.id, cluster.id);
	}
}
