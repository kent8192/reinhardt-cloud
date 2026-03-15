//! Migration definitions for nuages database tables.
//!
//! Provides `MigrationProvider` implementations for clusters and deployments tables.
//! Used by E2E tests to apply schema migrations on TestContainers PostgreSQL.

use reinhardt::db::migrations::{
	ColumnDefinition, FieldType, Migration, MigrationProvider, Operation,
};

/// Combined migration provider for all nuages apps.
pub struct NuagesMigrations;

impl MigrationProvider for NuagesMigrations {
	fn migrations() -> Vec<Migration> {
		vec![auth_users_initial(), clusters_initial(), deployments_initial()]
	}
}

/// Initial migration for the auth_users table.
fn auth_users_initial() -> Migration {
	Migration {
		app_label: "auth".to_string(),
		name: "0001_initial".to_string(),
		operations: vec![Operation::CreateTable {
			name: "auth_users".to_string(),
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
					name: "username".to_string(),
					type_definition: FieldType::VarChar(150),
					not_null: true,
					unique: true,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "email".to_string(),
					type_definition: FieldType::VarChar(254),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "password_hash".to_string(),
					type_definition: FieldType::Text,
					not_null: false,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "is_active".to_string(),
					type_definition: FieldType::Boolean,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: Some("true".to_string()),
				},
				ColumnDefinition {
					name: "last_login".to_string(),
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
				ColumnDefinition {
					name: "updated_at".to_string(),
					type_definition: FieldType::TimestampTz,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
			],
			constraints: vec![],
			without_rowid: None,
			interleave_in_parent: None,
			partition: None,
		}],
		dependencies: vec![],
		replaces: vec![],
		atomic: true,
		initial: Some(true),
		state_only: false,
		database_only: false,
		optional_dependencies: vec![],
		swappable_dependencies: vec![],
	}
}

/// Initial migration for the clusters table.
fn clusters_initial() -> Migration {
	Migration {
		app_label: "clusters".to_string(),
		name: "0001_initial".to_string(),
		operations: vec![Operation::CreateTable {
			name: "clusters".to_string(),
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
					name: "name".to_string(),
					type_definition: FieldType::VarChar(255),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "api_url".to_string(),
					type_definition: FieldType::VarChar(1024),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "is_active".to_string(),
					type_definition: FieldType::Boolean,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: Some("true".to_string()),
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
				ColumnDefinition {
					name: "updated_at".to_string(),
					type_definition: FieldType::TimestampTz,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
			],
			constraints: vec![],
			without_rowid: None,
			interleave_in_parent: None,
			partition: None,
		}],
		dependencies: vec![],
		replaces: vec![],
		atomic: true,
		initial: Some(true),
		state_only: false,
		database_only: false,
		optional_dependencies: vec![],
		swappable_dependencies: vec![],
	}
}

/// Initial migration for the deployments table.
fn deployments_initial() -> Migration {
	Migration {
		app_label: "deployments".to_string(),
		name: "0001_initial".to_string(),
		operations: vec![Operation::CreateTable {
			name: "deployments".to_string(),
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
					name: "app_name".to_string(),
					type_definition: FieldType::VarChar(255),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "cluster_id".to_string(),
					type_definition: FieldType::BigInteger,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "status".to_string(),
					type_definition: FieldType::VarChar(50),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: Some("pending".to_string()),
				},
				ColumnDefinition {
					name: "image".to_string(),
					type_definition: FieldType::VarChar(512),
					not_null: true,
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
				ColumnDefinition {
					name: "updated_at".to_string(),
					type_definition: FieldType::TimestampTz,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
			],
			constraints: vec![],
			without_rowid: None,
			interleave_in_parent: None,
			partition: None,
		}],
		dependencies: vec![],
		replaces: vec![],
		atomic: true,
		initial: Some(true),
		state_only: false,
		database_only: false,
		optional_dependencies: vec![],
		swappable_dependencies: vec![],
	}
}
