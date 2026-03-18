use reinhardt::db::migrations::prelude::*;
pub fn migration() -> Migration {
	Migration {
		app_label: "deployments".to_string(),
		name: "0003_add_cluster_id_fk".to_string(),
		operations: vec![Operation::AddConstraint {
			table: "deployments".to_string(),
			constraint: Constraint::ForeignKey {
				name: "deployments_cluster_id_fk".to_string(),
				columns: vec!["cluster_id".to_string()],
				referenced_table: "clusters".to_string(),
				referenced_columns: vec!["id".to_string()],
				on_delete: ForeignKeyAction::Restrict,
				on_update: ForeignKeyAction::Cascade,
				deferrable: None,
			},
		}],
		dependencies: vec![
			("deployments".to_string(), "0002_add_user_id".to_string()),
			("clusters".to_string(), "0002_add_user_id".to_string()),
		],
		atomic: true,
		replaces: vec![],
		initial: Some(false),
		state_only: false,
		database_only: false,
		swappable_dependencies: vec![],
		optional_dependencies: vec![],
	}
}
