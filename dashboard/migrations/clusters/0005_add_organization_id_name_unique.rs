// Composite UNIQUE constraint enforcing per-organization cluster name
// uniqueness. Mirrors the model-level `unique_together = ("organization_id",
// "name")` declaration in `apps::clusters::models::Cluster`.
//
// Hand-written because the CLI's `makemigrations` drops the
// `AddConstraint` operation. Root cause was localized via the
// diagnostic test
// `apps::clusters::tests::unit::test_cluster_model::tests::
//  diag_makemigrations_path_emits_add_constraint` — every layer up to
// and including `MigrationAutodetector::generate_operations()` emits
// the constraint correctly, but `generate_migrations()` (which the CLI
// calls) lacks the `added_constraints` / `removed_constraints` loops
// that PR #3998 added to `generate_operations()`. Tracked upstream as
// kent8192/reinhardt-web#4040 (downstream: reinhardt-cloud#443).
//
// Predecessor upstream fixes are in but each addressed a different
// layer: #3998 (autodetector consumer), #4024 (macro propagation),
// #4037 (lookup robustness). Replace this hand-written file once
// #4040 lands and `cargo run --bin manage -- makemigrations clusters`
// regenerates an equivalent `Operation::AddConstraint`.
//
// Constraint name matches the auto-generated name produced by the model
// macro (`{table}_{field1}_{field2}_uniq`) so that future autodetector-
// produced operations reconcile cleanly.

use reinhardt::db::migrations::prelude::*;
pub fn migration() -> Migration {
	Migration {
		app_label: "clusters".to_string(),
		name: "0005_add_organization_id_name_unique".to_string(),
		operations: vec![Operation::AddConstraint {
			table: "clusters".to_string(),
			constraint_sql:
				"CONSTRAINT clusters_organization_id_name_uniq UNIQUE (organization_id, name)"
					.to_string(),
		}],
		dependencies: vec![(
			"clusters".to_string(),
			"0004_replace_user_id_with_organization_id".to_string(),
		)],
		atomic: true,
		replaces: vec![],
		initial: Some(false),
		state_only: false,
		database_only: false,
		swappable_dependencies: vec![],
		optional_dependencies: vec![],
	}
}
