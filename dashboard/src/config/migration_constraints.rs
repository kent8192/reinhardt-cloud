//! Dashboard migration constraint registration.

use reinhardt::db::migrations::model_registry::global_registry;
use reinhardt::db::migrations::{ConstraintDefinition, MigrationError, Result};
use reinhardt::db::orm::Model;
use reinhardt::db::orm::inspection::ConstraintType;

use crate::apps::organizations::models::OrganizationMembership;

const MEMBERSHIP_ROLE_CHECK_NAME: &str = "organization_memberships_role_check";

/// Registers the model-declared membership role CHECK for migration generation.
pub fn register_membership_role_check() -> Result<()> {
	// Workaround for kent8192/reinhardt-web#6145 (tracked in
	// kent8192/reinhardt-cloud#869). Remove this workaround when the model
	// macro registers field CHECK metadata with the migration registry.
	//
	// Ideal implementation (without workaround):
	//   #[field(check = "...")] registers ConstraintDefinition directly.
	let role_check = OrganizationMembership::constraint_metadata()
		.into_iter()
		.find(|constraint| {
			constraint.name == "role_check" && constraint.constraint_type == ConstraintType::Check
		})
		.ok_or_else(|| {
			MigrationError::InvalidMigration(
				"OrganizationMembership.role CHECK metadata is missing".to_string(),
			)
		})?;
	let registry = global_registry();
	let mut membership = registry
		.get_model("organizations", "OrganizationMembership")
		.ok_or_else(|| {
			MigrationError::InvalidMigration(
				"OrganizationMembership migration metadata is missing".to_string(),
			)
		})?;
	let expected = ConstraintDefinition {
		// The field macro derives `role_check` for inspection metadata but does
		// not expose a physical CHECK constraint name. Preserve the existing
		// database contract while deriving its expression from the model.
		name: MEMBERSHIP_ROLE_CHECK_NAME.to_string(),
		constraint_type: "check".to_string(),
		fields: Vec::new(),
		expression: Some(role_check.definition),
		foreign_key_info: None,
	};
	let mut existing = membership
		.constraints()
		.iter()
		.filter(|constraint| constraint.name == expected.name);
	if let Some(constraint) = existing.next() {
		if constraint == &expected && existing.next().is_none() {
			return Ok(());
		}
		return Err(MigrationError::InvalidMigration(
			"OrganizationMembership.role CHECK migration metadata does not match the model"
				.to_string(),
		));
	}

	membership.add_constraint(expected);
	registry.register_model(membership);

	Ok(())
}

#[cfg(test)]
mod tests {
	use reinhardt::db::migrations::model_registry::ModelMetadata;
	use reinhardt::db::migrations::{ConstraintDefinition, MigrationError};
	use rstest::rstest;
	use serial_test::serial;

	use super::{MEMBERSHIP_ROLE_CHECK_NAME, global_registry, register_membership_role_check};

	struct MembershipRegistryRestore(Option<ModelMetadata>);

	impl MembershipRegistryRestore {
		fn capture() -> Self {
			Self(Some(
				global_registry()
					.get_model("organizations", "OrganizationMembership")
					.expect("organization membership migration metadata"),
			))
		}
	}

	impl Drop for MembershipRegistryRestore {
		fn drop(&mut self) {
			if let Some(metadata) = self.0.take() {
				global_registry().register_model(metadata);
			}
		}
	}

	#[rstest]
	#[serial(migration_registry)]
	fn membership_role_check_registration_is_idempotent() {
		// Arrange
		let _restore = MembershipRegistryRestore::capture();
		register_membership_role_check().expect("register membership role CHECK");

		// Act
		register_membership_role_check().expect("re-register membership role CHECK");
		let membership = global_registry()
			.get_model("organizations", "OrganizationMembership")
			.expect("organization membership migration metadata");
		let role_checks = membership
			.constraints()
			.iter()
			.filter(|constraint| constraint.name == MEMBERSHIP_ROLE_CHECK_NAME)
			.collect::<Vec<_>>();

		// Assert
		assert_eq!(role_checks.len(), 1);
		assert_eq!(role_checks[0].constraint_type, "check");
		assert!(role_checks[0].fields.is_empty());
		assert_eq!(
			role_checks[0].expression.as_deref(),
			Some("role IN ('owner', 'admin', 'developer', 'viewer')")
		);
	}

	#[rstest]
	#[serial(migration_registry)]
	fn membership_role_check_registration_rejects_mismatched_metadata() {
		// Arrange
		let _restore = MembershipRegistryRestore::capture();
		let registry = global_registry();
		let mut membership = registry
			.get_model("organizations", "OrganizationMembership")
			.expect("organization membership migration metadata");
		membership.add_constraint(ConstraintDefinition {
			name: MEMBERSHIP_ROLE_CHECK_NAME.to_string(),
			constraint_type: "check".to_string(),
			fields: Vec::new(),
			expression: Some("role <> ''".to_string()),
			foreign_key_info: None,
		});
		registry.register_model(membership);

		// Act
		let error = register_membership_role_check().expect_err("reject mismatched role CHECK");

		// Assert
		match error {
			MigrationError::InvalidMigration(message) => assert_eq!(
				message,
				"OrganizationMembership.role CHECK migration metadata does not match the model"
			),
			other => panic!("unexpected migration error: {other}"),
		}
	}
}
