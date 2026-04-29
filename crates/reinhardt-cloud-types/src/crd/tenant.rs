//! Multi-tenant ownership marker for `ReinhardtApp` resources.
//!
//! `TenantRef` identifies the owning Organization (and optionally a Team
//! within it) for a `ReinhardtApp` CR. The operator uses this reference to:
//!
//! - Compute a deterministic Kubernetes namespace per tenant
//!   (`tenant-{organization}` or `tenant-{organization}-{team}`)
//!   in which all owned resources are placed.
//! - Reject CRs whose `metadata.namespace` does not match the computed
//!   tenant namespace, surfacing a `Degraded` condition with reason
//!   `TenantMismatch`.
//! - Apply tenant-scoped `ResourceQuota` and `NetworkPolicy` resources
//!   that prevent one tenant from starving or addressing another.
//!
//! `TenantRef` is `Option`-wrapped on `ReinhardtAppSpec` for backward
//! compatibility with `v1alpha1`-style CRs that pre-date multi-tenancy.
//! When the field is absent the operator falls back to the legacy
//! "namespace owned externally" behavior.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::{ValidationError, validate_dns_1123_label};

/// Reference to the owning tenant of a `ReinhardtApp`.
///
/// The `organization` slug MUST match an `Organization.slug` in the
/// dashboard database (see issue #415). The optional `team` slug, when
/// present, narrows the tenancy to a `Team` belonging to that
/// `Organization`. Both slugs are validated as DNS-1123 labels so the
/// concatenated namespace name is always a valid Kubernetes identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TenantRef {
	/// Slug of the owning Organization (matches `Organization.slug` in
	/// the dashboard database).
	pub organization: String,

	/// Optional Team slug within the organization. When set, the tenant
	/// namespace becomes `tenant-{organization}-{team}` instead of
	/// `tenant-{organization}`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub team: Option<String>,
}

/// Namespace prefix applied to all operator-managed tenant namespaces.
///
/// Kept as a constant so the prefix can be referenced from both
/// validation logic (in this crate) and from the operator's namespace
/// reconciliation logic without duplicating a string literal.
pub const TENANT_NAMESPACE_PREFIX: &str = "tenant-";

impl TenantRef {
	/// Computes the deterministic Kubernetes namespace for this tenant.
	///
	/// Format:
	/// - `tenant-{organization}` when `team` is absent
	/// - `tenant-{organization}-{team}` when `team` is present
	///
	/// Callers should validate the `TenantRef` first via
	/// [`Self::validate`] to ensure the resulting namespace name is a
	/// valid DNS-1123 label.
	pub fn namespace(&self) -> String {
		match &self.team {
			Some(team) => format!("{TENANT_NAMESPACE_PREFIX}{}-{team}", self.organization),
			None => format!("{TENANT_NAMESPACE_PREFIX}{}", self.organization),
		}
	}

	/// Validates that both `organization` and (if present) `team` are
	/// DNS-1123 labels and that the computed namespace name fits within
	/// the 63-character Kubernetes label limit.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Err(e) = validate_dns_1123_label(&self.organization) {
			errors.push(ValidationError::new(format!(
				"tenant.organization {}",
				e.message
			)));
		}

		if let Some(ref team) = self.team
			&& let Err(e) = validate_dns_1123_label(team)
		{
			errors.push(ValidationError::new(format!("tenant.team {}", e.message)));
		}

		// The full namespace name is itself a DNS-1123 label and must
		// fit in 63 characters. Validate the composed value to catch
		// the case where org+team individually pass but together
		// overflow the limit.
		if errors.is_empty()
			&& let Err(e) = validate_dns_1123_label(&self.namespace())
		{
			errors.push(ValidationError::new(format!(
				"tenant namespace {} ({})",
				e.message,
				self.namespace()
			)));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn tenant_namespace_uses_org_only_when_team_absent() {
		// Arrange
		let tenant = TenantRef {
			organization: "acme".to_string(),
			team: None,
		};

		// Act
		let namespace = tenant.namespace();

		// Assert
		assert_eq!(namespace, "tenant-acme");
	}

	#[rstest]
	fn tenant_namespace_includes_team_when_present() {
		// Arrange
		let tenant = TenantRef {
			organization: "acme".to_string(),
			team: Some("platform".to_string()),
		};

		// Act
		let namespace = tenant.namespace();

		// Assert
		assert_eq!(namespace, "tenant-acme-platform");
	}

	#[rstest]
	fn tenant_validation_accepts_valid_org_only() {
		// Arrange
		let tenant = TenantRef {
			organization: "acme-prod".to_string(),
			team: None,
		};

		// Act
		let result = tenant.validate();

		// Assert
		assert!(result.is_ok(), "expected ok, got {result:?}");
	}

	#[rstest]
	fn tenant_validation_accepts_valid_org_and_team() {
		// Arrange
		let tenant = TenantRef {
			organization: "acme".to_string(),
			team: Some("platform-eng".to_string()),
		};

		// Act
		let result = tenant.validate();

		// Assert
		assert!(result.is_ok(), "expected ok, got {result:?}");
	}

	#[rstest]
	#[case("ACME", "tenant.organization")]
	#[case("acme_prod", "tenant.organization")]
	#[case("", "tenant.organization")]
	fn tenant_validation_rejects_invalid_organization(
		#[case] organization: &str,
		#[case] expected_prefix: &str,
	) {
		// Arrange
		let tenant = TenantRef {
			organization: organization.to_string(),
			team: None,
		};

		// Act
		let result = tenant.validate();

		// Assert
		let errors = result.expect_err("expected validation error");
		assert!(
			errors[0].message.starts_with(expected_prefix),
			"expected prefix {expected_prefix:?}, got {:?}",
			errors[0].message
		);
	}

	#[rstest]
	#[case("ACME")]
	#[case("team_one")]
	#[case("")]
	fn tenant_validation_rejects_invalid_team(#[case] team: &str) {
		// Arrange
		let tenant = TenantRef {
			organization: "acme".to_string(),
			team: Some(team.to_string()),
		};

		// Act
		let result = tenant.validate();

		// Assert
		let errors = result.expect_err("expected validation error");
		assert!(
			errors.iter().any(|e| e.message.starts_with("tenant.team")),
			"expected tenant.team error, got {errors:?}"
		);
	}

	#[rstest]
	fn tenant_validation_rejects_namespace_overflow() {
		// Arrange — both labels individually under 63 chars but the
		// concatenation `tenant-{org}-{team}` overflows.
		let tenant = TenantRef {
			organization: "a".repeat(40),
			team: Some("b".repeat(30)),
		};

		// Act
		let result = tenant.validate();

		// Assert
		let errors = result.expect_err("expected validation error");
		assert!(
			errors
				.iter()
				.any(|e| e.message.contains("tenant namespace")),
			"expected namespace overflow error, got {errors:?}"
		);
	}

	#[rstest]
	fn tenant_serialization_omits_team_when_none() {
		// Arrange
		let tenant = TenantRef {
			organization: "acme".to_string(),
			team: None,
		};

		// Act
		let json = serde_json::to_value(&tenant).expect("serialization should succeed");

		// Assert — `skip_serializing_if = "Option::is_none"` keeps wire
		// payload tight and avoids `team: null` noise in CR YAML.
		assert_eq!(json["organization"], "acme");
		assert!(
			json.get("team").is_none(),
			"team should be omitted: {json:?}"
		);
	}

	#[rstest]
	fn tenant_serialization_includes_team_when_present() {
		// Arrange
		let tenant = TenantRef {
			organization: "acme".to_string(),
			team: Some("platform".to_string()),
		};

		// Act
		let json = serde_json::to_value(&tenant).expect("serialization should succeed");

		// Assert
		assert_eq!(json["organization"], "acme");
		assert_eq!(json["team"], "platform");
	}

	#[rstest]
	fn tenant_deserialization_omits_team_when_absent() {
		// Arrange
		let json = r#"{"organization": "acme"}"#;

		// Act
		let tenant: TenantRef = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(tenant.organization, "acme");
		assert!(tenant.team.is_none());
	}

	#[rstest]
	fn tenant_deserialization_handles_team() {
		// Arrange
		let json = r#"{"organization": "acme", "team": "platform"}"#;

		// Act
		let tenant: TenantRef = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(tenant.organization, "acme");
		assert_eq!(tenant.team.as_deref(), Some("platform"));
	}
}
