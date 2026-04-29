// Composite UNIQUE constraint enforcing per-organization cluster name
// uniqueness. Mirrors the model-level `unique_together = ("organization_id",
// "name")` declaration in `apps::clusters::models::Cluster`.
//
// Hand-written because `cargo make makemigrations` does not regenerate
// this constraint even after kent8192/reinhardt-web#3989 — the autodetector
// `generate_operations` fix (PR #3998) landed in main, but re-running
// `makemigrations clusters --dry-run` against current main still proposes
// only `0005_alter_clusters_id` with no `AddConstraint`. The remaining
// gap appears to be on the macro side: `#[model(unique_together = ...)]`
// does not yet surface the constraint into the model state used for
// diffing. Tracked in reinhardt-cloud#443; a follow-up upstream issue
// is needed for the macro layer.
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
