use reinhardt::db::migrations::prelude::*;
pub fn migration() -> Migration {
	Migration {
		app_label: "auth".to_string(),
		name: "0002_add_email_unique".to_string(),
		operations: vec![Operation::AddConstraint {
			table: "auth_users".to_string(),
			constraint: Constraint::Unique {
				name: "auth_user_email_uniq".to_string(),
				columns: vec!["email".to_string()],
			},
		}],
		dependencies: vec![("auth".to_string(), "0001_initial".to_string())],
		atomic: true,
		replaces: vec![],
		initial: Some(false),
		state_only: false,
		database_only: false,
		swappable_dependencies: vec![],
		optional_dependencies: vec![],
	}
}
