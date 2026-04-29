// Composite UNIQUE constraint enforcing per-organization cluster name
// uniqueness. Mirrors the model-level `unique_together = ("organization_id",
// "name")` declaration in `apps::clusters::models::Cluster`.
//
// Hand-written because `cargo make makemigrations` does not regenerate
// this constraint. Two upstream fixes have landed but a third layer is
// still required before this file can be auto-generated:
//
// 1. kent8192/reinhardt-web#3989 / PR #3998 — autodetector consumer
//    iterates `DetectedChanges.added_constraints`. ✅ merged.
// 2. kent8192/reinhardt-web#4022 / PR #4024 — `#[model(unique_together)]`
//    macro propagates the constraint into `ModelMetadata`. ✅ merged.
// 3. kent8192/reinhardt-web#4032 — offline file-based state
//    reconstruction surfaces `unique_together` for the diff. ⏳ open.
//
// Tracked downstream as reinhardt-cloud#443. Re-run
// `cargo run --bin manage -- makemigrations clusters --dry-run` after
// #4032 lands; if the regenerated migration contains an equivalent
// `Operation::AddConstraint`, replace this hand-written file.
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
