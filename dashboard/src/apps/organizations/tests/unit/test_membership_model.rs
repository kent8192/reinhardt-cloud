use chrono::Utc;
use reinhardt::db::migrations::ForeignKeyAction;
use reinhardt::db::migrations::operations::{Constraint, Operation};
use reinhardt::db::orm::Model;
use reinhardt::db::orm::inspection::ConstraintType;
use rstest::rstest;
use serde_json;
use uuid::Uuid;

use crate::apps::organizations::models::OrganizationMembership;
use crate::apps::organizations::roles::MembershipRole;

mod organizations_initial_migration {
	include!(concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/migrations/organizations/0001_initial.rs"
	));
}

#[rstest]
fn membership_serializes_with_role_string() {
	// Arrange
	let user_id = Uuid::new_v4();
	let mut m = OrganizationMembership::build()
		.organization(42)
		.user(user_id)
		.role(MembershipRole::Owner.as_db_str().to_string())
		.finish();
	m.id = Some(7);
	m.created_at = Utc::now();

	// Act
	let json = serde_json::to_string(&m).expect("serialize");
	let roundtripped: OrganizationMembership = serde_json::from_str(&json).expect("deserialize");

	// Assert
	assert_eq!(roundtripped.organization_id(), 42);
	assert_eq!(roundtripped.user_id(), user_id);
	assert_eq!(roundtripped.role, "owner");
	assert!(json.contains("\"role\":\"owner\""));
}

#[rstest]
fn membership_role_metadata_restricts_values() {
	// Arrange
	let constraints = OrganizationMembership::constraint_metadata();

	// Act
	let role_check = constraints
		.iter()
		.find(|constraint| constraint.constraint_type == ConstraintType::Check);

	// Assert
	let role_check = role_check.expect("membership role CHECK constraint");
	assert_eq!(role_check.name, "role_check");
	assert_eq!(
		role_check.definition,
		"role IN ('owner', 'admin', 'developer', 'viewer')"
	);
}

#[rstest]
fn organizations_initial_migration_contains_membership_role_check() {
	// Arrange
	let migration = organizations_initial_migration::migration();

	// Act
	let role_check = migration
		.operations
		.iter()
		.find_map(|operation| match operation {
			Operation::CreateTable {
				name, constraints, ..
			} if name == "organization_memberships" => {
				constraints.iter().find_map(|constraint| match constraint {
					Constraint::Check { name, expression }
						if name == "organization_memberships_role_check" =>
					{
						Some(expression.as_str())
					}
					_ => None,
				})
			}
			_ => None,
		});

	// Assert
	assert_eq!(
		role_check,
		Some("role IN ('owner', 'admin', 'developer', 'viewer')")
	);
}

#[rstest]
fn organizations_initial_migration_preserves_membership_uniqueness_and_fk_actions() {
	// Arrange
	let migration = organizations_initial_migration::migration();

	// Act
	let constraints = migration
		.operations
		.iter()
		.find_map(|operation| match operation {
			Operation::CreateTable {
				name, constraints, ..
			} if name == "organization_memberships" => Some(constraints),
			_ => None,
		})
		.expect("organization_memberships table must be created");
	let unique_constraints = constraints
		.iter()
		.filter_map(|constraint| match constraint {
			Constraint::Unique { name, columns } => Some((name.clone(), columns.clone())),
			_ => None,
		})
		.collect::<Vec<_>>();
	let mut foreign_key_actions = constraints
		.iter()
		.filter_map(|constraint| match constraint {
			Constraint::ForeignKey {
				columns,
				on_delete,
				on_update,
				..
			} => Some((columns.clone(), *on_delete, *on_update)),
			_ => None,
		})
		.collect::<Vec<_>>();
	foreign_key_actions.sort();

	// Assert
	assert_eq!(
		unique_constraints,
		vec![(
			"organization_memberships_org_user_unique".to_string(),
			vec!["organization_id".to_string(), "user_id".to_string()],
		)]
	);
	assert_eq!(
		foreign_key_actions,
		vec![
			(
				vec!["organization_id".to_string()],
				ForeignKeyAction::Cascade,
				ForeignKeyAction::NoAction,
			),
			(
				vec!["user_id".to_string()],
				ForeignKeyAction::Cascade,
				ForeignKeyAction::NoAction,
			),
		]
	);
}
