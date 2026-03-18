use reinhardt::db::migrations::FieldType;
use reinhardt::db::migrations::prelude::*;
pub fn migration() -> Migration {
	Migration {
		app_label: "deployments".to_string(),
		name: "0003_merge_0001_initial_0002_add_cluster_id_fk_0_and_more".to_string(),
		operations: vec![],
		dependencies: vec![
			("deployments".to_string(), "0001_initial".to_string()),
			(
				"deployments".to_string(),
				"0002_add_cluster_id_fk".to_string(),
			),
			("deployments".to_string(), "0002_add_user_id".to_string()),
		],
		atomic: true,
		replaces: vec![],
		initial: None,
		state_only: false,
		database_only: false,
		swappable_dependencies: vec![],
		optional_dependencies: vec![],
	}
}
