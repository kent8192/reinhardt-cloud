use reinhardt::db::migrations::FieldType;
use reinhardt::db::migrations::prelude::*;
pub fn migration() -> Migration {
	Migration {
		app_label: "auth".to_string(),
		name: "0004_add_email_verification_tokens".to_string(),
		operations: vec![
			Operation::CreateTable {
				name: "auth_email_verification_tokens".to_string(),
				columns: vec![
					ColumnDefinition {
						name: "id".to_string(),
						type_definition: FieldType::BigInteger,
						not_null: true,
						unique: false,
						primary_key: true,
						auto_increment: true,
						default: None,
					},
					ColumnDefinition {
						name: "user_id".to_string(),
						type_definition: FieldType::Uuid,
						not_null: true,
						unique: false,
						primary_key: false,
						auto_increment: false,
						default: None,
					},
					ColumnDefinition {
						name: "pending_email".to_string(),
						type_definition: FieldType::VarChar(254u32),
						not_null: true,
						unique: false,
						primary_key: false,
						auto_increment: false,
						default: None,
					},
					ColumnDefinition {
						name: "token_hash".to_string(),
						type_definition: FieldType::VarChar(64u32),
						not_null: true,
						unique: true,
						primary_key: false,
						auto_increment: false,
						default: None,
					},
					ColumnDefinition {
						name: "expires_at".to_string(),
						type_definition: FieldType::TimestampTz,
						not_null: true,
						unique: false,
						primary_key: false,
						auto_increment: false,
						default: None,
					},
					ColumnDefinition {
						name: "consumed_at".to_string(),
						type_definition: FieldType::TimestampTz,
						not_null: false,
						unique: false,
						primary_key: false,
						auto_increment: false,
						default: None,
					},
					ColumnDefinition {
						name: "created_at".to_string(),
						type_definition: FieldType::TimestampTz,
						not_null: true,
						unique: false,
						primary_key: false,
						auto_increment: false,
						default: None,
					},
				],
				constraints: vec![Constraint::Unique {
					name: "auth_evt_token_hash_uniq".to_string(),
					columns: vec!["token_hash".to_string()],
				}],
				without_rowid: None,
				interleave_in_parent: None,
				partition: None,
			},
			// Composite index to accelerate lookups for a user's unconsumed tokens.
			Operation::AddConstraint {
				table: "auth_email_verification_tokens".to_string(),
				constraint_sql:
					"CONSTRAINT auth_evt_user_consumed_idx UNIQUE (user_id, token_hash)"
						.to_string(),
			},
		],
		dependencies: vec![("auth".to_string(), "0003_initial".to_string())],
		atomic: true,
		replaces: vec![],
		initial: None,
		state_only: false,
		database_only: false,
		swappable_dependencies: vec![],
		optional_dependencies: vec![],
	}
}
