// Workaround for reinhardt-web MigrationAutodetector spurious-migration bug
// (tracked in kent8192/reinhardt-cloud#719,
// upstream: kent8192/reinhardt-web#5367).
//
// This migration is hand-extracted from `cargo make makemigrations` output:
// only the `auth_api_keys` CreateTable is kept. The generator co-emits
// spurious operations for already-applied schema:
//   - AlterColumn `auth_permission.id`, `auth_group.id`,
//     `auth_social_accounts.id` to `Uuid` (already `Uuid` since `0003`)
//   - AlterColumn `auth_users.email`/`first_name`/`is_active`/`is_staff`/
//     `is_superuser`/`last_name` (re-asserting existing defaults/constraints)
//   - AddConstraint `auth_user_email_uniq` (already added in `0002`)
// This is a regression of reinhardt-web#475 on the 0.3 line. The production
// schema is correct; only the generator misdetects drift.
//
// Remove this workaround and regenerate normally via
// `cargo make makemigrations` once the upstream spurious-migration bug is
// fixed. The ideal generated file would contain only this CreateTable
// operation with no manual edits.

use reinhardt::db::migrations::FieldType;
use reinhardt::db::migrations::prelude::*;
pub fn migration() -> Migration {
	Migration {
		app_label: "auth".to_string(),
		name: "0007_add_api_keys".to_string(),
		operations: vec![Operation::CreateTable {
			name: "auth_api_keys".to_string(),
			columns: vec![
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
					name: "expires_at".to_string(),
					type_definition: FieldType::TimestampTz,
					not_null: false,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
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
					name: "label".to_string(),
					type_definition: FieldType::VarChar(100u32),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "last_used_at".to_string(),
					type_definition: FieldType::TimestampTz,
					not_null: false,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "prefix".to_string(),
					type_definition: FieldType::VarChar(16u32),
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
				ColumnDefinition {
					name: "revoked_at".to_string(),
					type_definition: FieldType::TimestampTz,
					not_null: false,
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
					name: "user_id".to_string(),
					type_definition: FieldType::Uuid,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
					default: None,
				},
			],
			constraints: vec![Constraint::Unique {
				name: "auth_apikey_token_hash_uniq".to_string(),
				columns: vec!["token_hash".to_string()],
			}],
			without_rowid: None,
			interleave_in_parent: None,
			partition: None,
		}],
		dependencies: vec![("auth".to_string(), "0006_add_social_account_token_metadata".to_string())],
		atomic: true,
		replaces: vec![],
		initial: Some(false),
		state_only: false,
		database_only: false,
		swappable_dependencies: vec![],
		optional_dependencies: vec![],
	}
}
