use reinhardt::db::migrations::FieldType;
use reinhardt::db::migrations::prelude::*;
pub fn migration() -> Migration {
	Migration {
		app_label: "auth".to_string(),
		name: "0003_add_admin_fields".to_string(),
		operations: vec![
			Operation::RenameColumn {
				table: "auth_users".to_string(),
				old_name: "created_at".to_string(),
				new_name: "date_joined".to_string(),
			},
			Operation::AddColumn {
				table: "auth_users".to_string(),
				column: ColumnDefinition {
					name: "first_name".to_string(),
					type_definition: FieldType::VarChar(128u32),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: Some(DefaultValue::String("".to_string())),
				},
				mysql_options: None,
			},
			Operation::AddColumn {
				table: "auth_users".to_string(),
				column: ColumnDefinition {
					name: "last_name".to_string(),
					type_definition: FieldType::VarChar(128u32),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: Some(DefaultValue::String("".to_string())),
				},
				mysql_options: None,
			},
			Operation::AddColumn {
				table: "auth_users".to_string(),
				column: ColumnDefinition {
					name: "is_staff".to_string(),
					type_definition: FieldType::Boolean,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: Some(DefaultValue::Bool(false)),
				},
				mysql_options: None,
			},
			Operation::AddColumn {
				table: "auth_users".to_string(),
				column: ColumnDefinition {
					name: "is_superuser".to_string(),
					type_definition: FieldType::Boolean,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: Some(DefaultValue::Bool(false)),
				},
				mysql_options: None,
			},
		],
		dependencies: vec!["auth/0002_add_email_unique".to_string()],
		atomic: true,
		replaces: vec![],
		initial: Some(false),
		state_only: false,
		database_only: false,
		swappable_dependencies: vec![],
		optional_dependencies: vec![],
	}
}
