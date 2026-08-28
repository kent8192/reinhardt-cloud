//! Unit tests for the Deployment model.

#[cfg(test)]
mod tests {
	use reinhardt::db::migrations::ForeignKeyAction;
	use reinhardt::db::migrations::operations::{Constraint, Operation};
	use rstest::rstest;

	use crate::apps::deployments::models::Deployment;

	mod deployments_initial_migration {
		include!(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/migrations/deployments/0001_initial.rs"
		));
	}

	/// All fields from Deployment::new match constructor arguments.
	#[rstest]
	fn test_deployment_new_sets_fields() {
		// Arrange
		let organization_id: i64 = 42;
		let project_name = "my-app".to_string();
		let cluster_id = 42i64;
		let status = "pending".to_string();
		let image = "ghcr.io/my-app:latest".to_string();
		let project_yaml =
			"apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n".to_string();

		// Act
		let deployment = Deployment::build()
			.organization(organization_id)
			.project_name(project_name.clone())
			.cluster(cluster_id)
			.status(status.clone())
			.image(image.clone())
			.project_yaml(Some(project_yaml.clone()))
			.finish();

		// Assert
		assert_eq!(deployment.organization_id(), organization_id);
		assert_eq!(deployment.project_name, project_name);
		assert_eq!(deployment.cluster_id(), cluster_id);
		assert_eq!(deployment.status, status);
		assert_eq!(deployment.image, image);
		assert_eq!(deployment.project_yaml, Some(project_yaml));
	}

	/// Deployment::new sets id to None (auto-increment on insert).
	#[rstest]
	fn test_deployment_new_id_is_none() {
		// Arrange & Act
		let deployment = Deployment::build()
			.organization(1)
			.project_name("app".to_string())
			.cluster(1)
			.status("pending".to_string())
			.image("nginx:latest".to_string())
			.project_yaml(None)
			.finish();

		// Assert
		assert_eq!(deployment.id, None);
	}

	/// Deployment accepts various status string values.
	#[rstest]
	#[case("pending")]
	#[case("running")]
	#[case("failed")]
	#[case("succeeded")]
	fn test_deployment_status_values(#[case] status: &str) {
		// Arrange & Act
		let deployment = Deployment::build()
			.organization(1)
			.project_name("app".to_string())
			.cluster(1)
			.status(status.to_string())
			.image("nginx:latest".to_string())
			.project_yaml(None)
			.finish();

		// Assert
		assert_eq!(deployment.status, status);
	}

	/// Deployment survives a serde_json roundtrip.
	#[rstest]
	fn test_deployment_serialization_roundtrip() {
		// Arrange
		let mut deployment = Deployment::build()
			.organization(7)
			.project_name("roundtrip-app".to_string())
			.cluster(99)
			.status("running".to_string())
			.image("ghcr.io/roundtrip:v1".to_string())
			.project_yaml(None)
			.finish();
		deployment.id = Some(7);

		// Act
		let json = serde_json::to_string(&deployment).expect("serialize");
		let restored: Deployment = serde_json::from_str(&json).expect("deserialize");

		// Assert
		assert_eq!(restored.id, deployment.id);
		assert_eq!(restored.organization_id(), deployment.organization_id());
		assert_eq!(restored.project_name, deployment.project_name);
		assert_eq!(restored.cluster_id(), deployment.cluster_id());
		assert_eq!(restored.status, deployment.status);
		assert_eq!(restored.image, deployment.image);
		assert_eq!(restored.project_yaml, deployment.project_yaml);
	}

	#[rstest]
	fn test_deployments_initial_migration_uses_final_project_columns() {
		// Arrange
		let migration = deployments_initial_migration::migration();

		// Act
		let columns = migration
			.operations
			.iter()
			.find_map(|operation| match operation {
				Operation::CreateTable { name, columns, .. } if name == "deployments" => {
					Some(columns)
				}
				_ => None,
			})
			.expect("deployments table must be created");
		let project_columns = columns
			.iter()
			.filter(|column| matches!(column.name.as_str(), "project_name" | "project_yaml"))
			.map(|column| {
				(
					column.name.as_str(),
					format!("{:?}", column.type_definition),
					column.not_null,
					column.unique,
					column.default.as_deref(),
				)
			})
			.collect::<Vec<_>>();

		// Assert
		assert_eq!(migration.app_label, "deployments");
		assert_eq!(migration.name, "0001_initial");
		assert_eq!(
			project_columns,
			vec![
				(
					"project_name",
					"VarChar(255)".to_string(),
					true,
					false,
					None,
				),
				(
					"project_yaml",
					"VarChar(65535)".to_string(),
					false,
					false,
					None,
				),
			]
		);
		assert!(
			!columns
				.iter()
				.any(|column| matches!(column.name.as_str(), "app_name" | "reinhardt_app_yaml"))
		);
	}

	#[rstest]
	fn test_deployments_initial_migration_preserves_foreign_key_actions() {
		// Arrange
		let migration = deployments_initial_migration::migration();

		// Act
		let mut foreign_key_actions = migration
			.operations
			.iter()
			.find_map(|operation| match operation {
				Operation::CreateTable {
					name, constraints, ..
				} if name == "deployments" => Some(
					constraints
						.iter()
						.filter_map(|constraint| match constraint {
							Constraint::ForeignKey {
								columns,
								on_delete,
								on_update,
								..
							} => Some((columns.clone(), *on_delete, *on_update)),
							_ => None,
						})
						.collect::<Vec<_>>(),
				),
				_ => None,
			})
			.expect("deployments table must be created");
		foreign_key_actions.sort();

		// Assert
		assert_eq!(
			foreign_key_actions,
			vec![
				(
					vec!["cluster_id".to_string()],
					ForeignKeyAction::Restrict,
					ForeignKeyAction::Cascade,
				),
				(
					vec!["organization_id".to_string()],
					ForeignKeyAction::Cascade,
					ForeignKeyAction::NoAction,
				),
			]
		);
	}
}
