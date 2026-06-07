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
		let cluster = Cluster::build()
			.organization(organization_id)
			.name(name.clone())
			.api_url(api_url.clone())
			.is_active(true)
			.token_hash(None)
			.token_last_rotated_at(None)
			.finish();

		// Assert
		assert_eq!(*cluster.organization_id(), organization_id);
		assert_eq!(cluster.name, name);
		assert_eq!(cluster.api_url, api_url);
		assert!(cluster.is_active);
	}

	#[rstest]
	fn test_cluster_new_id_is_none() {
		// Arrange
		let organization_id: i64 = 7;

		// Act
		let cluster = Cluster::build()
			.organization(organization_id)
			.name("test-cluster".to_string())
			.api_url("https://k8s.example.com:6443".to_string())
			.is_active(true)
			.token_hash(None)
			.token_last_rotated_at(None)
			.finish();

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
		let cluster = Cluster::build()
			.organization(organization_id)
			.name("flag-test".to_string())
			.api_url("https://k8s.example.com:6443".to_string())
			.is_active(active)
			.token_hash(None)
			.token_last_rotated_at(None)
			.finish();

		// Assert
		assert_eq!(cluster.is_active, active);
	}

	/// Diagnostic for reinhardt-cloud#443 / reinhardt-web#4038: verify
	/// that the `#[model(unique_together = ...)]` macro's
	/// `metadata.add_constraint(...)` calls actually populate
	/// `ModelMetadata.constraints` in the migration registry that
	/// `makemigrations` reads. Failure here would localize the bug to
	/// the macro / `#[ctor]` registration path, before any autodetector
	/// logic runs.
	#[rstest]
	fn diag_cluster_metadata_carries_unique_together_constraint() {
		// Arrange — force the `#[ctor]` registration block to fire by
		// referring to the type, then read the migration registry.
		let _ensure_registered = std::any::type_name::<Cluster>();
		let registry = reinhardt::db::migrations::model_registry::global_registry();

		// Act
		let metadata = registry
			.get_model("clusters", "Cluster")
			.expect("Cluster must be registered in the migration model registry");
		let constraint_count = metadata.constraints().len();
		let names: Vec<_> = metadata
			.constraints()
			.iter()
			.map(|c| c.name.clone())
			.collect();

		// Assert — if this fails, the macro side (PR #4024) is not
		// reaching ModelMetadata.constraints in our build, and any
		// downstream "fix" past the macro is moot.
		assert!(
			constraint_count > 0,
			"ModelMetadata.constraints is empty for Cluster. Expected the \
			 #[model(unique_together)] macro to have emitted an \
			 add_constraint(...) call. Registered constraint names: {names:?}"
		);
		assert!(
			names
				.iter()
				.any(|n| n == "clusters_organization_id_name_uniq"),
			"Expected clusters_organization_id_name_uniq in ModelMetadata.constraints, got {names:?}"
		);

		// Now check the next layer: to_model_state() must carry constraints
		// into the ModelState the autodetector sees.
		let to_state_clusters = metadata.to_model_state();
		let state_names: Vec<_> = to_state_clusters
			.constraints
			.iter()
			.map(|c| c.name.clone())
			.collect();
		assert!(
			state_names
				.iter()
				.any(|n| n == "clusters_organization_id_name_uniq"),
			"to_model_state() did not carry the unique_together constraint into \
			 ModelState. ModelMetadata.constraints={names:?}, ModelState.constraints={state_names:?}"
		);

		// Final layer: feed a synthetic from_state (model present, no
		// constraints — mirrors what offline reconstruction produces from
		// 0001_initial's CreateTable) and target_state (the real one) into
		// the autodetector. If AddConstraint shows up here, the bug is in
		// the offline state reconstructor; if it does NOT, the bug is in
		// the autodetector itself.
		use reinhardt::db::migrations::operations::Operation;
		use reinhardt::db::migrations::{MigrationAutodetector, ModelState, ProjectState};

		let mut from_clusters = ModelState::new("clusters", "Clusters");
		from_clusters.table_name = "clusters".to_string();
		// Mirror the same fields target has, with no constraints.
		for f in to_state_clusters.fields.values() {
			from_clusters.add_field(f.clone());
		}

		let mut from_state = ProjectState::new();
		from_state.add_model(from_clusters);

		let mut to_state = ProjectState::new();
		to_state.add_model(to_state_clusters);

		let detector = MigrationAutodetector::new(from_state, to_state);
		let ops = detector.generate_operations();
		let add_constraint_count = ops
			.iter()
			.filter(|op| matches!(op, Operation::AddConstraint { .. }))
			.count();
		let op_summaries: Vec<String> = ops
			.iter()
			.map(|op| format!("{op:?}").chars().take(80).collect::<String>())
			.collect();
		assert!(
			add_constraint_count > 0,
			"Autodetector emitted no AddConstraint despite target_state \
			 carrying the unique_together constraint. ops={op_summaries:?}"
		);
	}

	/// Diagnostic for reinhardt-cloud#443: run the EXACT makemigrations CLI
	/// path — `ProjectState::from_global_registry()` → `filter_by_app` →
	/// `MigrationAutodetector::generate_migrations` — against a synthetic
	/// from_state. If this still loses the AddConstraint, the bug is
	/// localized to either `from_global_registry` or `filter_by_app` /
	/// `generate_migrations` (since we already proved single-model
	/// to_model_state preserves the constraint).
	#[rstest]
	fn diag_makemigrations_path_emits_add_constraint() {
		// Arrange — force registration, then build target_state exactly as
		// MakeMigrationsCommand does.
		let _ensure_registered = std::any::type_name::<Cluster>();
		let target_project_state = reinhardt::db::migrations::ProjectState::from_global_registry();
		let app_target_state = target_project_state.filter_by_app("clusters");

		// Sanity: the post-filter target must still carry the constraint.
		let target_cluster = app_target_state
			.models
			.get(&("clusters".to_string(), "Cluster".to_string()))
			.expect("Cluster model in filter_by_app('clusters') target");
		let target_constraint_names: Vec<_> = target_cluster
			.constraints
			.iter()
			.map(|c| c.name.clone())
			.collect();
		assert!(
			target_constraint_names
				.iter()
				.any(|n| n == "clusters_organization_id_name_uniq"),
			"After from_global_registry → filter_by_app, target Cluster has \
			 no unique_together. constraints={target_constraint_names:?}"
		);

		// Build a synthetic from_state mimicking what offline file-based
		// state reconstruction produces from 0001_initial: same table,
		// PascalCase plural model name, identical fields, no constraints.
		let mut from_clusters = reinhardt::db::migrations::ModelState::new("clusters", "Clusters");
		from_clusters.table_name = "clusters".to_string();
		for f in target_cluster.fields.values() {
			from_clusters.add_field(f.clone());
		}
		let mut app_from_state = reinhardt::db::migrations::ProjectState::new();
		app_from_state.add_model(from_clusters);

		// Act — call BOTH generate_operations() (lower-level, used by our
		// earlier diag) AND generate_migrations() (what the CLI calls).
		let detector =
			reinhardt::db::migrations::MigrationAutodetector::new(app_from_state, app_target_state);
		let direct_ops = detector.generate_operations();
		let migrations = detector.generate_migrations();
		let migration_ops: Vec<_> = migrations
			.iter()
			.flat_map(|m| m.operations.iter())
			.collect();

		let kind = |op: &reinhardt::db::migrations::operations::Operation| {
			let s = format!("{op:?}");
			s.split('{').next().unwrap_or("?").trim().to_string()
		};
		let direct_kinds: Vec<String> = direct_ops.iter().map(kind).collect();
		let migration_kinds: Vec<String> = migration_ops.iter().map(|op| kind(op)).collect();

		// Diagnostic dump — shows the divergence even when assertions
		// fail.
		eprintln!("[diag] generate_operations() ops: {direct_kinds:?}");
		eprintln!("[diag] generate_migrations() ops: {migration_kinds:?}");

		let direct_has_add = direct_ops.iter().any(|op| {
			matches!(
				op,
				reinhardt::db::migrations::operations::Operation::AddConstraint { .. }
			)
		});
		let migration_has_add = migration_ops.iter().any(|op| {
			matches!(
				op,
				reinhardt::db::migrations::operations::Operation::AddConstraint { .. }
			)
		});

		// Assert — these two should agree; if generate_operations sees the
		// constraint but generate_migrations does not, the gap is in
		// generate_migrations itself.
		assert_eq!(
			direct_has_add, migration_has_add,
			"generate_operations() and generate_migrations() disagree on \
			 AddConstraint emission. direct={direct_kinds:?} migration={migration_kinds:?}"
		);
		assert!(
			migration_has_add,
			"Mimicked makemigrations CLI path emitted no AddConstraint. \
			 direct={direct_kinds:?} migration={migration_kinds:?}"
		);
	}

	/// Regression test that locked in the offline-state-reconstruction bug
	/// originally tracked as reinhardt-web#4052 (a residual after
	/// reinhardt-web#4050 closed reinhardt-web#4049): when `from_state` is
	/// built via `ProjectState::apply_migration_operations` from a
	/// `CreateTable` op against a model with a `table_name` override that
	/// matches `0001_initial`'s table, the autodetector used to emit a no-op
	/// `Operation::AlterColumn { old_definition: None, .. }` for the
	/// unchanged `id` PK. The root cause was that the `#[model]` macro
	/// emitted `null = "true"` on `Option<T>` primary keys while
	/// `column_def_to_field_state` (driven by `not_null = true`) reported
	/// `nullable = false`. The direct `nullable != nullable` check in
	/// `has_field_changed` then tripped before #4050's canonicalization
	/// could absorb the asymmetry.
	///
	/// reinhardt-web#4053 suppresses the `null = true` emission for
	/// `Option<T>` PKs, restoring symmetric `nullable` between the
	/// migration-replay `from_state` and the macro-registry `to_state`.
	/// This test exercises the same scenario the CLI runs and asserts no
	/// spurious `AlterColumn` for the unchanged `id` PK.
	#[rstest]
	fn diag_apply_migration_operations_from_state_no_spurious_altercolumn() {
		// Arrange — build the to_state via the same code path the CLI uses.
		let _ensure_registered = std::any::type_name::<Cluster>();
		let target_project_state = reinhardt::db::migrations::ProjectState::from_global_registry();
		let app_target_state = target_project_state.filter_by_app("clusters");

		// Mirror migrations/clusters/0001_initial.rs: a single CreateTable
		// with the six initial columns. Apply it through
		// `apply_migration_operations` so `from_state` is populated via
		// `column_def_to_field_state` — the production path that produces
		// sparse `params` HashMaps.
		use reinhardt::db::migrations::FieldType;
		use reinhardt::db::migrations::operations::{ColumnDefinition, Operation};
		let create_clusters = Operation::CreateTable {
			name: "clusters".to_string(),
			columns: vec![
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
					name: "created_at".to_string(),
					type_definition: FieldType::TimestampTz,
					not_null: true,
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
					name: "is_active".to_string(),
					type_definition: FieldType::Boolean,
					not_null: true,
					unique: false,
					primary_key: false,
					auto_increment: false,
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
		};
		let mut app_from_state = reinhardt::db::migrations::ProjectState::new();
		app_from_state.apply_migration_operations(&[create_clusters], "clusters");

		// Act — run the same autodetector entry points the CLI hits.
		let detector =
			reinhardt::db::migrations::MigrationAutodetector::new(app_from_state, app_target_state);
		let direct_ops = detector.generate_operations();
		let migrations = detector.generate_migrations();
		let migration_ops: Vec<_> = migrations
			.iter()
			.flat_map(|m| m.operations.iter())
			.collect();

		let alter_id_in_direct: Vec<_> = direct_ops
			.iter()
			.filter(|op| {
				matches!(
					op,
					Operation::AlterColumn { column, .. } if column == "id"
				)
			})
			.collect();
		let alter_id_in_migration: Vec<_> = migration_ops
			.iter()
			.filter(|op| {
				matches!(
					op,
					Operation::AlterColumn { column, .. } if column == "id"
				)
			})
			.collect();

		// Assert — the unchanged `id` PK must NOT surface as AlterColumn
		// from either entry point. This regression-guards the upstream
		// residual bug fixed by reinhardt-web#4053 (closed
		// reinhardt-cloud#476); the migration in
		// 0005_add_constraint_clusters.rs is now the verbatim
		// autogenerated output.
		assert!(
			alter_id_in_direct.is_empty(),
			"generate_operations() emitted spurious AlterColumn for unchanged \
			 `id` PK under apply_migration_operations from_state. ops={direct_ops:?}"
		);
		assert!(
			alter_id_in_migration.is_empty(),
			"generate_migrations() emitted spurious AlterColumn for unchanged \
			 `id` PK under apply_migration_operations from_state. ops={migration_ops:?}"
		);
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
		let cluster = Cluster::build()
			.organization(99)
			.name("roundtrip".to_string())
			.api_url("https://k8s.example.com:6443".to_string())
			.is_active(true)
			.token_hash(None)
			.token_last_rotated_at(None)
			.finish();

		// Act
		let json = serde_json::to_string(&cluster).expect("serialize should succeed");
		let deserialized: Cluster =
			serde_json::from_str(&json).expect("deserialize should succeed");

		// Assert
		assert_eq!(deserialized.name, cluster.name);
		assert_eq!(deserialized.api_url, cluster.api_url);
		assert_eq!(deserialized.organization_id(), cluster.organization_id());
		assert_eq!(deserialized.is_active, cluster.is_active);
		assert_eq!(deserialized.id, cluster.id);
	}
}
