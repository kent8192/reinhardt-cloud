// Composite UNIQUE constraint enforcing per-organization cluster name
// uniqueness. Mirrors the model-level `unique_together = ("organization_id",
// "name")` declaration in `apps::clusters::models::Cluster`.
//
// Hand-written because `cargo make makemigrations` does not regenerate
// this constraint. The autodetector `generate_operations` fix landed
// upstream (kent8192/reinhardt-web#3989, PR #3998), but the macro side
// — `#[model(unique_together = ...)]` propagating the constraint into
// `ModelMetadata` for diffing — is still pending. Tracked upstream as
// kent8192/reinhardt-web#4022 (downstream: reinhardt-cloud#443).
//
// Remove this hand-written file once #4022 lands and re-running
// `makemigrations clusters --dry-run` regenerates an `AddConstraint`
// equivalent to the operation below.
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
