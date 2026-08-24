//! Reinhardt Project Management CLI for Reinhardt Cloud
//!
//! This is the project-specific management command interface (equivalent to Django's manage.py).
//!
//! ## Router Registration
//!
//! URL patterns are automatically registered by the framework.
//! No manual registration is required - see `src/config/urls.rs` for the
//! `#[routes]` attribute macro that enables this.
//!
//! ## Native vs. WASM
//!
//! This binary is native-only (it depends on `tokio`, `reinhardt::commands`,
//! and other server-side crates that don't link for `wasm32-unknown-unknown`).
//! The WASM build of this crate skips it via the `cfg(not(target_arch =
//! "wasm32"))` gate below. The empty wasm32 stub keeps `wasm-pack test`'s
//! `cargo build --tests` happy without dragging native deps into the wasm
//! target. Refs `kent8192/reinhardt-cloud#574`.

#[cfg(not(target_arch = "wasm32"))]
use reinhardt::commands::{CommandRegistry, execute_from_command_line_with_registry_and_settings};
#[cfg(not(target_arch = "wasm32"))]
use reinhardt::db::migrations::{
	ColumnDefinition, Constraint, FilesystemRepository, FilesystemSource, Migration,
	MigrationError, MigrationRenderOptions, MigrationSource, Operation, Result as MigrationResult,
	global_registry,
};
#[cfg(not(target_arch = "wasm32"))]
use reinhardt_cloud_dashboard::config::management::{
	CreateApiTokenCommand, ListApiTokensCommand, RevokeApiTokenCommand, SeedSelfDeployUserCommand,
};
#[cfg(not(target_arch = "wasm32"))]
use reinhardt_cloud_dashboard::config::migration_constraints::register_membership_role_check;
#[cfg(not(target_arch = "wasm32"))]
use reinhardt_cloud_dashboard::config::settings::get_settings;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs, process};

#[cfg(not(target_arch = "wasm32"))]
type MigrationIdentity = (String, String);

#[cfg(not(target_arch = "wasm32"))]
type MigrationFileSnapshot = BTreeMap<PathBuf, String>;

#[cfg(not(target_arch = "wasm32"))]
fn migration_output_path() -> Option<PathBuf> {
	let arguments = std::env::args().skip(1).collect::<Vec<_>>();
	if arguments.first().map(String::as_str) != Some("makemigrations")
		|| arguments
			.iter()
			.any(|argument| argument == "--dry-run" || argument == "--check")
	{
		return None;
	}

	arguments
		.iter()
		.find_map(|argument| argument.strip_prefix("--migrations-dir="))
		.map(PathBuf::from)
		.or_else(|| {
			arguments
				.windows(2)
				.find(|arguments| arguments[0] == "--migrations-dir")
				.map(|arguments| PathBuf::from(&arguments[1]))
		})
		.or_else(|| Some(PathBuf::from("migrations")))
}

#[cfg(not(target_arch = "wasm32"))]
fn is_migration_file(path: &Path) -> bool {
	path.extension().and_then(|extension| extension.to_str()) == Some("rs")
		&& path
			.file_stem()
			.and_then(|stem| stem.to_str())
			.is_some_and(|stem| stem.starts_with(|character: char| character.is_ascii_digit()))
}

#[cfg(not(target_arch = "wasm32"))]
fn snapshot_migration_directory(
	root: &Path,
	directory: &Path,
	snapshot: &mut MigrationFileSnapshot,
) -> MigrationResult<()> {
	for entry in fs::read_dir(directory)? {
		let entry = entry?;
		let path = entry.path();
		if entry.file_type()?.is_dir() {
			snapshot_migration_directory(root, &path, snapshot)?;
		} else if entry.file_type()?.is_file() && is_migration_file(&path) {
			let relative_path = path.strip_prefix(root).map_err(|_| {
				MigrationError::InvalidMigration(format!(
					"migration file {} is outside {}",
					path.display(),
					root.display()
				))
			})?;
			snapshot.insert(relative_path.to_path_buf(), fs::read_to_string(path)?);
		}
	}
	Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn migration_file_snapshot(root: &Path) -> MigrationResult<MigrationFileSnapshot> {
	let mut snapshot = MigrationFileSnapshot::new();
	if root.exists() {
		snapshot_migration_directory(root, root, &mut snapshot)?;
	}
	Ok(snapshot)
}

#[cfg(not(target_arch = "wasm32"))]
fn migration_identity_from_path(path: &Path) -> Option<MigrationIdentity> {
	let std::path::Component::Normal(app_label) = path.components().next()? else {
		return None;
	};
	let app_label = app_label.to_str()?;
	let name = path.file_stem()?.to_str()?;
	Some((app_label.to_string(), name.to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
fn changed_migration_identities(
	before: &MigrationFileSnapshot,
	after: &MigrationFileSnapshot,
) -> MigrationResult<BTreeSet<MigrationIdentity>> {
	let mut identities = BTreeSet::new();
	for (path, source) in after {
		if before.get(path).is_some_and(|previous| previous == source) {
			continue;
		}
		let identity = migration_identity_from_path(path).ok_or_else(|| {
			MigrationError::InvalidMigration(format!(
				"cannot determine migration identity from {}",
				path.display()
			))
		})?;
		identities.insert(identity);
	}
	Ok(identities)
}

#[cfg(not(target_arch = "wasm32"))]
fn referenced_tables(operation: &Operation) -> Vec<&str> {
	let Operation::CreateTable { constraints, .. } = operation else {
		return Vec::new();
	};

	constraints
		.iter()
		.filter_map(|constraint| match constraint {
			Constraint::ForeignKey {
				referenced_table, ..
			}
			| Constraint::OneToOne {
				referenced_table, ..
			} => Some(referenced_table.as_str()),
			_ => None,
		})
		.collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn introduced_columns(operation: &Operation) -> Vec<(&str, &str)> {
	match operation {
		Operation::CreateTable { name, columns, .. } => columns
			.iter()
			.map(|column| (name.as_str(), column.name.as_str()))
			.collect(),
		Operation::AddColumn { table, column, .. } => {
			vec![(table.as_str(), column.name.as_str())]
		}
		_ => Vec::new(),
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn registered_foreign_key_indexes() -> MigrationResult<BTreeSet<(String, Vec<String>)>> {
	Ok(global_registry()
		.try_get_models()?
		.into_iter()
		.flat_map(|model| {
			model.fields.into_iter().filter_map(move |(name, field)| {
				(field.foreign_key.is_some()
					&& field.params.get("db_index").map(String::as_str) == Some("true"))
				.then(|| (model.table_name.clone(), vec![name]))
			})
		})
		.collect())
}

#[cfg(not(target_arch = "wasm32"))]
fn active_email_verification_token_index() -> Operation {
	// Workaround for kent8192/reinhardt-web#6152 (tracked in
	// kent8192/reinhardt-cloud#874). Remove this workaround when model metadata
	// supports a declarative non-unique partial index.
	//
	// Ideal implementation (without workaround):
	//   #[field(index = true, condition = "consumed_at IS NULL")]
	Operation::CreateIndex {
		table: "auth_email_verification_tokens".to_string(),
		columns: vec!["user_id".to_string()],
		unique: false,
		index_type: None,
		where_clause: Some("consumed_at IS NULL".to_string()),
		concurrently: false,
		expressions: None,
		mysql_options: None,
		operator_class: None,
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn has_active_email_verification_token_index(operations: &[Operation]) -> bool {
	operations.iter().any(|operation| {
		matches!(
			operation,
			Operation::CreateIndex {
				table,
				columns,
				unique: false,
				where_clause: Some(predicate),
				..
			} if table == "auth_email_verification_tokens"
				&& columns.len() == 1
				&& columns[0] == "user_id"
				&& predicate == "consumed_at IS NULL"
		)
	})
}

#[cfg(not(target_arch = "wasm32"))]
fn validate_generated_unique_layout(
	table: &str,
	columns: &[ColumnDefinition],
	constraints: &[Constraint],
	column: &str,
	expected_constraint_name: &str,
) -> MigrationResult<()> {
	let matching_columns = columns
		.iter()
		.filter(|definition| definition.name == column)
		.collect::<Vec<_>>();
	if matching_columns.len() != 1 || !matching_columns[0].unique {
		return Err(MigrationError::InvalidMigration(format!(
			"generated migration must define exactly one unique column {table}.{column}"
		)));
	}

	let matching_constraint_names = constraints
		.iter()
		.filter_map(|constraint| match constraint {
			Constraint::Unique { name, columns } if columns.len() == 1 && columns[0] == column => {
				Some(name.as_str())
			}
			_ => None,
		})
		.collect::<Vec<_>>();
	if matching_constraint_names.len() != 1
		|| matching_constraint_names[0] != expected_constraint_name
	{
		return Err(MigrationError::InvalidMigration(format!(
			"generated migration must define exactly one unique constraint {expected_constraint_name} on {table}({column})"
		)));
	}

	Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_historical_unique_layout(operation: &mut Operation) -> MigrationResult<()> {
	// Workaround for kent8192/reinhardt-web#6160 (tracked in
	// kent8192/reinhardt-cloud#878). Remove this workaround when makemigrations
	// preserves the historical column and table unique-constraint layout while
	// creating a fresh initial migration.
	//
	// Ideal implementation (without workaround):
	//   MakeMigrationsCommand emits the historical unique flags and constraint names.
	let Operation::CreateTable {
		name: table,
		columns,
		constraints,
		..
	} = operation
	else {
		return Ok(());
	};

	match table.as_str() {
		"auth_users" => {
			validate_generated_unique_layout(
				table,
				columns,
				constraints,
				"email",
				"auth_user_email_uniq",
			)?;
			let email = columns
				.iter_mut()
				.find(|column| column.name == "email")
				.ok_or_else(|| {
					MigrationError::InvalidMigration(
						"generated migration must define auth_users.email".to_string(),
					)
				})?;
			email.unique = false;
		}
		"auth_email_verification_tokens" => {
			validate_generated_unique_layout(
				table,
				columns,
				constraints,
				"token_hash",
				"auth_emailverificationtoken_token_hash_uniq",
			)?;
			let constraint = constraints
				.iter_mut()
				.find(|constraint| {
					matches!(
						constraint,
						Constraint::Unique { name, columns }
							if name == "auth_emailverificationtoken_token_hash_uniq"
								&& columns.len() == 1
								&& columns[0] == "token_hash"
					)
				})
				.ok_or_else(|| {
					MigrationError::InvalidMigration(
						"generated migration must define auth_emailverificationtoken_token_hash_uniq"
							.to_string(),
					)
				})?;
			if let Constraint::Unique { name, .. } = constraint {
				*name = "auth_evt_token_hash_uniq".to_string();
			}
		}
		"organizations" => remove_redundant_generated_unique_constraint(
			table,
			columns,
			constraints,
			"slug",
			"organizations_organization_slug_uniq",
		)?,
		"github_installations" => remove_redundant_generated_unique_constraint(
			table,
			columns,
			constraints,
			"installation_id",
			"github_githubinstallation_installation_id_uniq",
		)?,
		"github_repositories" => remove_redundant_generated_unique_constraint(
			table,
			columns,
			constraints,
			"github_repository_id",
			"github_githubrepository_github_repository_id_uniq",
		)?,
		_ => {}
	}

	Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn remove_redundant_generated_unique_constraint(
	table: &str,
	columns: &[ColumnDefinition],
	constraints: &mut Vec<Constraint>,
	column: &str,
	expected_constraint_name: &str,
) -> MigrationResult<()> {
	validate_generated_unique_layout(
		table,
		columns,
		constraints,
		column,
		expected_constraint_name,
	)?;
	constraints.retain(|constraint| {
		!matches!(
			constraint,
			Constraint::Unique { name, columns }
				if name == expected_constraint_name
					&& columns.len() == 1
					&& columns[0] == column
		)
	});
	Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
fn normalize_migrations(
	migrations: &mut [Migration],
	foreign_key_indexes: &BTreeSet<(String, Vec<String>)>,
) -> MigrationResult<()> {
	let generated_migrations = migrations
		.iter()
		.map(|migration| (migration.app_label.clone(), migration.name.clone()))
		.collect::<BTreeSet<_>>();
	normalize_selected_migrations(migrations, &generated_migrations, foreign_key_indexes)
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_selected_migrations(
	migrations: &mut [Migration],
	generated_migrations: &BTreeSet<MigrationIdentity>,
	foreign_key_indexes: &BTreeSet<(String, Vec<String>)>,
) -> MigrationResult<()> {
	let mut table_owners = BTreeMap::new();
	for migration in migrations.iter() {
		for operation in &migration.operations {
			let Operation::CreateTable { name, .. } = operation else {
				continue;
			};
			let owner = (migration.app_label.clone(), migration.name.clone());
			if let Some(existing) = table_owners.insert(name.clone(), owner.clone())
				&& existing != owner
			{
				return Err(MigrationError::InvalidMigration(format!(
					"table {name} is created by both {}.{} and {}.{}",
					existing.0, existing.1, owner.0, owner.1
				)));
			}
		}
	}

	for migration in migrations {
		let identity = (migration.app_label.clone(), migration.name.clone());
		if !generated_migrations.contains(&identity) {
			continue;
		}
		let local_tables = migration
			.operations
			.iter()
			.filter_map(|operation| match operation {
				Operation::CreateTable { name, .. } => Some(name.clone()),
				_ => None,
			})
			.collect::<BTreeSet<_>>();
		let mut create_tables = Vec::new();
		let mut remaining = Vec::new();
		for operation in std::mem::take(&mut migration.operations) {
			if matches!(operation, Operation::CreateTable { .. }) {
				create_tables.push(operation);
			} else {
				remaining.push(operation);
			}
		}

		let mut created = BTreeSet::new();
		let mut ordered = Vec::with_capacity(create_tables.len() + remaining.len());
		while !create_tables.is_empty() {
			let next = create_tables.iter().position(|operation| {
				let Operation::CreateTable { name, .. } = operation else {
					return false;
				};
				referenced_tables(operation).into_iter().all(|referenced| {
					referenced == name
						|| !local_tables.contains(referenced)
						|| created.contains(referenced)
				})
			});
			let Some(next) = next else {
				let tables = create_tables
					.iter()
					.filter_map(|operation| match operation {
						Operation::CreateTable { name, .. } => Some(name.as_str()),
						_ => None,
					})
					.collect::<Vec<_>>()
					.join(", ");
				return Err(MigrationError::CircularDependency {
					cycle: format!("inline foreign keys among tables: {tables}"),
				});
			};
			let operation = create_tables.remove(next);
			if let Operation::CreateTable { name, .. } = &operation {
				created.insert(name.clone());
			}
			ordered.push(operation);
		}
		ordered.append(&mut remaining);
		migration.operations = ordered;
		for operation in &mut migration.operations {
			normalize_historical_unique_layout(operation)?;
		}

		let introduced_columns = migration
			.operations
			.iter()
			.flat_map(introduced_columns)
			.map(|(table, column)| (table.to_string(), column.to_string()))
			.collect::<BTreeSet<_>>();
		let introduces_active_email_verification_token_user =
			introduced_columns.iter().any(|(table, column)| {
				table == "auth_email_verification_tokens" && column == "user_id"
			});
		let mut indexes = BTreeSet::new();
		for operation in &migration.operations {
			match operation {
				Operation::CreateTable {
					name,
					columns,
					constraints,
					..
				} => {
					for column in columns.iter().filter(|column| column.unique) {
						indexes.insert((name.clone(), vec![column.name.clone()]));
					}
					for constraint in constraints {
						match constraint {
							Constraint::Unique { columns, .. } => {
								indexes.insert((name.clone(), columns.clone()));
							}
							Constraint::OneToOne { column, .. } => {
								indexes.insert((name.clone(), vec![column.clone()]));
							}
							_ => {}
						}
					}
				}
				Operation::CreateIndex {
					table,
					columns,
					index_type: None,
					where_clause: None,
					expressions: None,
					mysql_options: None,
					operator_class: None,
					..
				} => {
					indexes.insert((table.clone(), columns.clone()));
				}
				_ => {}
			}
		}
		migration.operations.extend(
			foreign_key_indexes
				.iter()
				.filter(|(table, columns)| {
					columns
						.iter()
						.all(|column| introduced_columns.contains(&(table.clone(), column.clone())))
				})
				.filter(|index| !indexes.contains(*index))
				.cloned()
				.map(|(table, columns)| Operation::CreateIndex {
					table,
					columns,
					unique: false,
					index_type: None,
					where_clause: None,
					concurrently: false,
					expressions: None,
					mysql_options: None,
					operator_class: None,
				}),
		);
		if introduces_active_email_verification_token_user
			&& !has_active_email_verification_token_index(&migration.operations)
		{
			migration
				.operations
				.push(active_email_verification_token_index());
		}

		for referenced in migration.operations.iter().flat_map(referenced_tables) {
			if let Some(owner) = table_owners.get(referenced)
				&& owner != &identity
			{
				migration.dependencies.push(owner.clone());
			}
		}
		migration.dependencies.sort();
		migration.dependencies.dedup();
	}

	Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn normalize_generated_migrations(
	root: &Path,
	before: &MigrationFileSnapshot,
) -> MigrationResult<usize> {
	// Workaround for kent8192/reinhardt-web#6146 and #6147 (tracked in
	// kent8192/reinhardt-cloud#870 and #871). Remove this workaround when
	// makemigrations orders inline foreign keys, emits cross-app initial
	// dependencies, and preserves db_index metadata.
	//
	// Ideal implementation (without workaround):
	//   MakeMigrationsCommand writes dependency-ordered migrations and indexes directly.
	let after = migration_file_snapshot(root)?;
	let generated_migrations = changed_migration_identities(before, &after)?;
	if generated_migrations.is_empty() {
		return Ok(0);
	}

	let source = FilesystemSource::new(root);
	let mut migrations = source.all_migrations().await?;
	let available_migrations = migrations
		.iter()
		.map(|migration| (migration.app_label.clone(), migration.name.clone()))
		.collect::<BTreeSet<_>>();
	if let Some((app_label, name)) = generated_migrations
		.difference(&available_migrations)
		.next()
	{
		return Err(MigrationError::InvalidMigration(format!(
			"generated migration {app_label}.{name} could not be loaded"
		)));
	}
	normalize_selected_migrations(
		&mut migrations,
		&generated_migrations,
		&registered_foreign_key_indexes()?,
	)?;

	let repository = FilesystemRepository::new(root);
	let rendered = migrations
		.iter()
		.filter(|migration| {
			generated_migrations.contains(&(migration.app_label.clone(), migration.name.clone()))
		})
		.map(|migration| {
			let path = root
				.join(&migration.app_label)
				.join(format!("{}.rs", migration.name));
			let source = repository.render(
				migration,
				MigrationRenderOptions {
					include_header: true,
				},
			)?;
			Ok((path, source))
		})
		.collect::<MigrationResult<Vec<_>>>()?;

	let mut changed = 0;
	for (path, source) in rendered {
		if fs::read_to_string(&path)? != source {
			fs::write(path, source)?;
			changed += 1;
		}
	}
	Ok(changed)
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
	// SAFETY: Called at program start before any spawned tasks.
	// env::set_var is safe in single-threaded context.
	unsafe {
		std::env::set_var(
			"REINHARDT_SETTINGS_MODULE",
			"reinhardt_cloud_dashboard.config.settings",
		);
	}
	if let Err(error) = register_membership_role_check() {
		eprintln!("Error: {error}");
		process::exit(1);
	}

	let mut registry = CommandRegistry::new();
	registry.register(Box::new(SeedSelfDeployUserCommand));
	registry.register(Box::new(CreateApiTokenCommand));
	registry.register(Box::new(ListApiTokensCommand));
	registry.register(Box::new(RevokeApiTokenCommand));
	let generated_migration_snapshot = match migration_output_path() {
		Some(path) => match migration_file_snapshot(&path) {
			Ok(snapshot) => Some((path, snapshot)),
			Err(error) => {
				eprintln!("Error: {error}");
				process::exit(1);
			}
		},
		None => None,
	};

	if let Err(e) =
		execute_from_command_line_with_registry_and_settings(registry, get_settings()).await
	{
		eprintln!("Error: {e}");
		process::exit(1);
	}
	if let Some((path, snapshot)) = generated_migration_snapshot {
		match normalize_generated_migrations(&path, &snapshot).await {
			Ok(0) => {}
			Ok(changed) => println!("Normalized {changed} generated migration file(s)"),
			Err(error) => {
				eprintln!("Error: {error}");
				process::exit(1);
			}
		}
	}
}

/// WASM stub. The dashboard's WASM bundle is built via `cdylib` from
/// `src/lib.rs` (`#[wasm_bindgen(start)]` in `client::wasm_entry::main`),
/// not from this CLI. Refs #574.
#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod migration_normalization_tests {
	use reinhardt::db::migrations::{
		ColumnDefinition, Constraint, FieldType, ForeignKeyAction, Migration, MigrationError,
		Operation, Result,
	};
	use rstest::rstest;

	use super::{
		changed_migration_identities, normalize_migrations, normalize_selected_migrations,
	};
	use std::collections::{BTreeMap, BTreeSet};
	use std::path::PathBuf;

	fn create_table(name: &str, referenced_table: Option<&str>) -> Operation {
		let columns = referenced_table.map_or_else(Vec::new, |_| {
			vec![reinhardt::db::migrations::ColumnDefinition::new(
				"owner_id",
				reinhardt::db::migrations::FieldType::BigInteger,
			)]
		});
		let constraints = referenced_table.map_or_else(Vec::new, |referenced_table| {
			vec![Constraint::ForeignKey {
				name: format!("fk_{name}"),
				columns: vec!["owner_id".to_string()],
				referenced_table: referenced_table.to_string(),
				referenced_columns: vec!["id".to_string()],
				on_delete: ForeignKeyAction::NoAction,
				on_update: ForeignKeyAction::NoAction,
				deferrable: None,
			}]
		});
		Operation::CreateTable {
			name: name.to_string(),
			columns,
			constraints,
			without_rowid: None,
			interleave_in_parent: None,
			partition: None,
		}
	}

	fn add_column(table: &str, name: &str) -> Operation {
		Operation::AddColumn {
			table: table.to_string(),
			column: reinhardt::db::migrations::ColumnDefinition::new(
				name,
				reinhardt::db::migrations::FieldType::BigInteger,
			),
			mysql_options: None,
		}
	}

	fn unique_table(table: &str, column: &str, constraint_name: &str) -> Operation {
		let mut definition = ColumnDefinition::new(column, FieldType::VarChar(255));
		definition.unique = true;
		Operation::CreateTable {
			name: table.to_string(),
			columns: vec![definition],
			constraints: vec![Constraint::Unique {
				name: constraint_name.to_string(),
				columns: vec![column.to_string()],
			}],
			without_rowid: None,
			interleave_in_parent: None,
			partition: None,
		}
	}

	fn unique_layout<'a>(
		migration: &'a Migration,
		table: &str,
	) -> (&'a [ColumnDefinition], &'a [Constraint]) {
		migration
			.operations
			.iter()
			.find_map(|operation| match operation {
				Operation::CreateTable {
					name,
					columns,
					constraints,
					..
				} if name == table => Some((columns.as_slice(), constraints.as_slice())),
				_ => None,
			})
			.expect("test table")
	}

	fn unique_constraint_names(constraints: &[Constraint]) -> Vec<&str> {
		constraints
			.iter()
			.filter_map(|constraint| match constraint {
				Constraint::Unique { name, .. } => Some(name.as_str()),
				_ => None,
			})
			.collect()
	}

	fn invalid_layout_message(migration: Migration) -> String {
		let mut migrations = vec![migration];
		match normalize_migrations(&mut migrations, &BTreeSet::new()) {
			Err(MigrationError::InvalidMigration(message)) => message,
			Err(error) => panic!("unexpected migration error: {error}"),
			Ok(()) => panic!("normalization unexpectedly succeeded"),
		}
	}

	#[rstest]
	fn orders_local_foreign_keys_and_adds_cross_app_dependencies() -> Result<()> {
		// Arrange
		let mut auth = Migration::new("0001_initial", "auth");
		auth.operations = vec![
			create_table("auth_tokens", Some("auth_users")),
			unique_table("auth_users", "email", "auth_user_email_uniq"),
		];
		let mut organizations = Migration::new("0001_initial", "organizations");
		organizations.operations = vec![create_table("tenant_organizations", Some("auth_users"))];
		let mut migrations = vec![organizations, auth];
		let indexes = BTreeSet::from([("auth_tokens".to_string(), vec!["owner_id".to_string()])]);

		// Act
		normalize_migrations(&mut migrations, &indexes)?;

		// Assert
		let auth = migrations
			.iter()
			.find(|migration| migration.app_label == "auth")
			.expect("auth migration");
		let auth_tables = auth
			.operations
			.iter()
			.filter_map(|operation| match operation {
				Operation::CreateTable { name, .. } => Some(name.as_str()),
				_ => None,
			})
			.collect::<Vec<_>>();
		assert_eq!(auth_tables, ["auth_users", "auth_tokens"]);
		let auth_indexes = auth
			.operations
			.iter()
			.filter_map(|operation| match operation {
				Operation::CreateIndex { table, columns, .. } => {
					Some((table.as_str(), columns.as_slice()))
				}
				_ => None,
			})
			.collect::<Vec<_>>();
		assert_eq!(
			auth_indexes,
			[("auth_tokens", ["owner_id".to_string()].as_slice())]
		);
		let organizations = migrations
			.iter()
			.find(|migration| migration.app_label == "organizations")
			.expect("organizations migration");
		assert_eq!(
			organizations.dependencies,
			[("auth".to_string(), "0001_initial".to_string())]
		);
		assert!(
			organizations
				.operations
				.iter()
				.all(|operation| !matches!(operation, Operation::CreateIndex { .. }))
		);

		Ok(())
	}

	#[rstest]
	fn adds_a_foreign_key_index_to_the_migration_that_introduces_its_column() -> Result<()> {
		// Arrange
		let mut initial = Migration::new("0001_initial", "auth");
		initial.operations = vec![create_table("test_users", None)];
		let mut add_profile = Migration::new("0002_add_profile", "auth");
		add_profile.operations = vec![add_column("test_users", "profile_id")];
		let mut migrations = vec![initial, add_profile];
		let indexes = BTreeSet::from([("test_users".to_string(), vec!["profile_id".to_string()])]);

		// Act
		normalize_migrations(&mut migrations, &indexes)?;

		// Assert
		assert!(
			migrations[0]
				.operations
				.iter()
				.all(|operation| !matches!(operation, Operation::CreateIndex { .. }))
		);
		assert!(matches!(
			migrations[1].operations.last(),
			Some(Operation::CreateIndex { table, columns, .. })
				if table == "test_users" && columns == &vec!["profile_id".to_string()]
		));

		Ok(())
	}

	#[rstest]
	fn only_normalizes_new_migration_when_an_existing_foreign_key_gains_an_index() -> Result<()> {
		// Arrange
		let mut initial = Migration::new("0001_initial", "auth");
		initial.operations = vec![
			create_table("auth_tokens", Some("auth_users")),
			create_table("auth_users", None),
		];
		let initial_operations = initial.operations.clone();
		let mut add_profile = Migration::new("0002_add_profile", "auth");
		add_profile.operations = vec![add_column("auth_users", "profile_id")];
		let mut migrations = vec![initial, add_profile];
		let generated_migrations =
			BTreeSet::from([("auth".to_string(), "0002_add_profile".to_string())]);
		let indexes = BTreeSet::from([
			("auth_tokens".to_string(), vec!["owner_id".to_string()]),
			("auth_users".to_string(), vec!["profile_id".to_string()]),
		]);

		// Act
		normalize_selected_migrations(&mut migrations, &generated_migrations, &indexes)?;

		// Assert
		assert_eq!(migrations[0].operations, initial_operations);
		assert!(migrations[0].dependencies.is_empty());
		assert!(matches!(
			migrations[1].operations.last(),
			Some(Operation::CreateIndex { table, columns, .. })
				if table == "auth_users" && columns == &vec!["profile_id".to_string()]
		));

		Ok(())
	}

	#[rstest]
	fn identifies_only_new_or_generator_changed_migrations() -> Result<()> {
		// Arrange
		let before = BTreeMap::from([
			(PathBuf::from("auth/0001_initial.rs"), "initial".to_string()),
			(
				PathBuf::from("clusters/0001_initial.rs"),
				"initial".to_string(),
			),
		]);
		let after = BTreeMap::from([
			(PathBuf::from("auth/0001_initial.rs"), "initial".to_string()),
			(PathBuf::from("auth/0002_add_profile.rs"), "new".to_string()),
			(
				PathBuf::from("clusters/0001_initial.rs"),
				"changed".to_string(),
			),
		]);

		// Act
		let changed = changed_migration_identities(&before, &after)?;

		// Assert
		assert_eq!(
			changed,
			BTreeSet::from([
				("auth".to_string(), "0002_add_profile".to_string()),
				("clusters".to_string(), "0001_initial".to_string()),
			])
		);

		Ok(())
	}

	#[rstest]
	fn adds_the_active_email_token_partial_index_without_treating_a_full_index_as_equivalent()
	-> Result<()> {
		// Arrange
		let mut migration = Migration::new("0002_add_email_token_user", "auth");
		migration.operations = vec![
			add_column("auth_email_verification_tokens", "user_id"),
			Operation::CreateIndex {
				table: "auth_email_verification_tokens".to_string(),
				columns: vec!["user_id".to_string()],
				unique: false,
				index_type: None,
				where_clause: None,
				concurrently: false,
				expressions: None,
				mysql_options: None,
				operator_class: None,
			},
		];
		let mut migrations = vec![migration];

		// Act
		normalize_migrations(&mut migrations, &BTreeSet::new())?;
		normalize_migrations(&mut migrations, &BTreeSet::new())?;

		// Assert
		let predicates = migrations[0]
			.operations
			.iter()
			.filter_map(|operation| match operation {
				Operation::CreateIndex {
					table,
					columns,
					unique: false,
					where_clause,
					..
				} if table == "auth_email_verification_tokens"
					&& columns.len() == 1
					&& columns[0] == "user_id" =>
				{
					Some(where_clause.as_deref())
				}
				_ => None,
			})
			.collect::<Vec<_>>();
		assert_eq!(predicates, [None, Some("consumed_at IS NULL")]);

		Ok(())
	}

	#[rstest]
	fn preserves_auth_user_email_as_table_unique_only() -> Result<()> {
		// Arrange
		let mut migration = Migration::new("0001_initial", "auth");
		let mut users = unique_table("auth_users", "email", "auth_user_email_uniq");
		let Operation::CreateTable {
			columns,
			constraints,
			..
		} = &mut users
		else {
			unreachable!("unique_table returns CreateTable");
		};
		let mut username = ColumnDefinition::new("username", FieldType::VarChar(255));
		username.unique = true;
		columns.push(username);
		constraints.push(Constraint::Unique {
			name: "auth_user_username_uniq".to_string(),
			columns: vec!["username".to_string()],
		});
		migration.operations = vec![users];
		let mut migrations = vec![migration];

		// Act
		normalize_migrations(&mut migrations, &BTreeSet::new())?;

		// Assert
		let (columns, constraints) = unique_layout(&migrations[0], "auth_users");
		assert_eq!(
			columns
				.iter()
				.find(|column| column.name == "email")
				.expect("email column")
				.unique,
			false
		);
		assert_eq!(
			columns
				.iter()
				.find(|column| column.name == "username")
				.expect("username column")
				.unique,
			true
		);
		assert_eq!(
			unique_constraint_names(constraints),
			["auth_user_email_uniq", "auth_user_username_uniq"]
		);

		Ok(())
	}

	#[rstest]
	fn preserves_email_verification_token_dual_unique_with_historical_name() -> Result<()> {
		// Arrange
		let mut migration = Migration::new("0001_initial", "auth");
		migration.operations = vec![unique_table(
			"auth_email_verification_tokens",
			"token_hash",
			"auth_emailverificationtoken_token_hash_uniq",
		)];
		let mut migrations = vec![migration];

		// Act
		normalize_migrations(&mut migrations, &BTreeSet::new())?;

		// Assert
		let (columns, constraints) =
			unique_layout(&migrations[0], "auth_email_verification_tokens");
		assert_eq!(columns[0].unique, true);
		assert_eq!(
			unique_constraint_names(constraints),
			["auth_evt_token_hash_uniq"]
		);

		Ok(())
	}

	#[rstest]
	fn removes_only_historical_redundant_single_column_unique_constraints() -> Result<()> {
		// Arrange
		let mut migration = Migration::new("0001_initial", "dashboard");
		migration.operations = vec![
			unique_table(
				"organizations",
				"slug",
				"organizations_organization_slug_uniq",
			),
			unique_table(
				"github_installations",
				"installation_id",
				"github_githubinstallation_installation_id_uniq",
			),
			unique_table(
				"github_repositories",
				"github_repository_id",
				"github_githubrepository_github_repository_id_uniq",
			),
			unique_table("auth_test_users", "username", "auth_user_username_uniq"),
			unique_table("auth_groups", "name", "auth_group_name_uniq"),
			unique_table("auth_api_keys", "token_hash", "auth_apikey_token_hash_uniq"),
			unique_table(
				"github_projects",
				"repository_id",
				"github_projects_repository_id_key",
			),
		];
		let mut migrations = vec![migration];

		// Act
		normalize_migrations(&mut migrations, &BTreeSet::new())?;

		// Assert
		for (table, column) in [
			("organizations", "slug"),
			("github_installations", "installation_id"),
			("github_repositories", "github_repository_id"),
		] {
			let (columns, constraints) = unique_layout(&migrations[0], table);
			assert_eq!(columns[0].name, column);
			assert_eq!(columns[0].unique, true);
			assert!(unique_constraint_names(constraints).is_empty());
		}
		for (table, expected_name) in [
			("auth_test_users", "auth_user_username_uniq"),
			("auth_groups", "auth_group_name_uniq"),
			("auth_api_keys", "auth_apikey_token_hash_uniq"),
			("github_projects", "github_projects_repository_id_key"),
		] {
			let (columns, constraints) = unique_layout(&migrations[0], table);
			assert_eq!(columns[0].unique, true);
			assert_eq!(unique_constraint_names(constraints), [expected_name]);
		}

		Ok(())
	}

	#[rstest]
	fn does_not_rewrite_historical_unique_constraints() -> Result<()> {
		// Arrange
		let mut historical = Migration::new("0001_initial", "auth");
		historical.operations = vec![
			unique_table("auth_users", "email", "auth_user_email_uniq"),
			unique_table(
				"auth_email_verification_tokens",
				"token_hash",
				"auth_emailverificationtoken_token_hash_uniq",
			),
		];
		let historical_operations = historical.operations.clone();
		let mut generated = Migration::new("0002_add_slug", "organizations");
		generated.operations = vec![unique_table(
			"organizations",
			"slug",
			"organizations_organization_slug_uniq",
		)];
		let mut migrations = vec![historical, generated];
		let generated_migrations =
			BTreeSet::from([("organizations".to_string(), "0002_add_slug".to_string())]);

		// Act
		normalize_selected_migrations(&mut migrations, &generated_migrations, &BTreeSet::new())?;

		// Assert
		assert_eq!(migrations[0].operations, historical_operations);
		let (columns, constraints) = unique_layout(&migrations[1], "organizations");
		assert_eq!(columns[0].unique, true);
		assert!(unique_constraint_names(constraints).is_empty());

		Ok(())
	}

	#[rstest]
	fn rejects_missing_or_malformed_generated_unique_layouts() {
		// Arrange
		let mut missing_constraint = Migration::new("0001_initial", "auth");
		let mut missing_table = unique_table("auth_users", "email", "auth_user_email_uniq");
		let Operation::CreateTable { constraints, .. } = &mut missing_table else {
			panic!("test table must be a CreateTable operation");
		};
		constraints.clear();
		missing_constraint.operations = vec![missing_table];

		let mut unexpected_constraint = Migration::new("0001_initial", "auth");
		unexpected_constraint.operations = vec![unique_table(
			"auth_users",
			"email",
			"auth_user_email_unexpected_uniq",
		)];

		let mut duplicate_constraint = Migration::new("0001_initial", "auth");
		let mut duplicate_table = unique_table("auth_users", "email", "auth_user_email_uniq");
		let Operation::CreateTable { constraints, .. } = &mut duplicate_table else {
			panic!("test table must be a CreateTable operation");
		};
		constraints.push(Constraint::Unique {
			name: "auth_user_email_duplicate_uniq".to_string(),
			columns: vec!["email".to_string()],
		});
		duplicate_constraint.operations = vec![duplicate_table];

		let mut malformed_column = Migration::new("0001_initial", "auth");
		let mut malformed_table = unique_table("auth_users", "email", "auth_user_email_uniq");
		let Operation::CreateTable { columns, .. } = &mut malformed_table else {
			panic!("test table must be a CreateTable operation");
		};
		columns[0].unique = false;
		malformed_column.operations = vec![malformed_table];

		// Act and Assert
		let expected_constraint_error = "generated migration must define exactly one unique constraint auth_user_email_uniq on auth_users(email)";
		assert_eq!(
			invalid_layout_message(missing_constraint),
			expected_constraint_error
		);
		assert_eq!(
			invalid_layout_message(unexpected_constraint),
			expected_constraint_error
		);
		assert_eq!(
			invalid_layout_message(duplicate_constraint),
			expected_constraint_error
		);
		assert_eq!(
			invalid_layout_message(malformed_column),
			"generated migration must define exactly one unique column auth_users.email"
		);
	}
}
