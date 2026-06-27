//! Per-app workload `ServiceAccount` builder.
//!
//! Distinct from the operator-managed `{app-name}-storage` KSA used for
//! storage-backend access. This builder constructs the workload's own KSA,
//! which carries Workload Identity / IRSA annotations granting the
//! application pods cloud-API access (e.g. publishing to Pub/Sub, reading
//! from a Secret Manager, calling cloud KMS).
//!
//! Naming:
//! - When `create == true`, the workload KSA is named from
//!   `spec.service_account.name` or `{app-name}-app` when omitted.
//! - When `create == false`, the operator does not bind the workload to a
//!   user-supplied KSA name. The field is intentionally ignored because an
//!   app manifest must not be able to select an arbitrary same-namespace KSA.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::ServiceAccount;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::Project;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Resolves the name the per-app workload `ServiceAccount` should use.
///
/// Returns:
/// - `None` when `spec.service_account` is unset.
/// - `Some(name)` when `create == true` and the spec explicitly sets a name.
/// - `Some("{app}-app")` when `create == true` and no name was supplied.
/// - `None` when `create == false`, even if a name was supplied, so an app
///   manifest cannot bind workloads to arbitrary same-namespace KSAs.
pub(crate) fn resolved_sa_name(app: &Project) -> Option<String> {
	let spec = app.spec.service_account.as_ref()?;

	if !spec.create {
		return None;
	}

	if let Some(name) = spec.name.as_ref() {
		return Some(name.clone());
	}

	Some(format!("{}-app", app.name_any()))
}

/// Builds a per-app workload `ServiceAccount` resource.
///
/// Returns `Ok(None)` when:
/// - `spec.service_account` is unset, or
/// - `spec.service_account.create == false` (the operator does not create
///   or bind a workload KSA).
///
/// When `create == true`, returns the SA with:
/// - `metadata.name` per [`resolved_sa_name`]
/// - `metadata.annotations` carrying user-supplied Workload Identity /
///   IRSA bindings (or `None` when the annotations map is empty)
/// - `metadata.labels` set to the standard operator labels (web component)
/// - `metadata.owner_references` pointing at the owning `Project`
///   so deletion of the parent cascades to this KSA
pub(crate) fn build_service_account(app: &Project) -> Result<Option<ServiceAccount>, Error> {
	let Some(spec) = app.spec.service_account.as_ref() else {
		return Ok(None);
	};

	if !spec.create {
		return Ok(None);
	}

	// `resolved_sa_name` only returns `None` when `spec.service_account`
	// is `None`, which we already handled above. When `create == true`
	// we always get back either the explicit name or the `{app}-app`
	// fallback, so unwrapping here is safe — but we use `expect` to make
	// the invariant explicit in case the resolution rule changes later.
	let name = resolved_sa_name(app)
		.expect("resolved_sa_name returns Some when service_account is Some and create is true");

	let annotations: Option<BTreeMap<String, String>> = if spec.annotations.is_empty() {
		None
	} else {
		Some(spec.annotations.clone())
	};

	let namespace = super::require_namespace(app)?;
	let owner_ref = owner_reference(app)?;
	let labels = standard_labels(app, Component::Web);

	Ok(Some(ServiceAccount {
		metadata: ObjectMeta {
			name: Some(name),
			namespace: Some(namespace),
			labels: Some(labels),
			annotations,
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		..Default::default()
	}))
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ProjectSpec;
	use reinhardt_cloud_types::crd::service_account::ServiceAccountSpec;
	use rstest::rstest;

	fn make_test_app(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
				image: "img:v1".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_build_service_account_returns_none_when_spec_unset() {
		// Arrange
		let app = make_test_app("myapp");

		// Act
		let result = build_service_account(&app).expect("build should succeed");

		// Assert
		assert!(
			result.is_none(),
			"no spec.service_account → no SA materialized"
		);
	}

	#[rstest]
	fn test_build_service_account_returns_none_when_create_false() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: false,
			name: Some("user-managed-sa".to_string()),
			annotations: BTreeMap::new(),
		});

		// Act
		let result = build_service_account(&app).expect("build should succeed");

		// Assert
		assert!(
			result.is_none(),
			"create=false → user pre-creates the KSA; operator builds none"
		);
	}

	#[rstest]
	fn test_build_service_account_default_name_is_app_suffix() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: None,
			annotations: BTreeMap::new(),
		});

		// Act
		let sa = build_service_account(&app)
			.expect("build should succeed")
			.expect("create=true → SA materialized");

		// Assert
		assert_eq!(
			sa.metadata.name.as_deref(),
			Some("myapp-app"),
			"default name disambiguates from {{app}}-storage and from a user-managed KSA \
			 named after the app"
		);
	}

	#[rstest]
	fn test_build_service_account_uses_explicit_name() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: Some("custom-sa".to_string()),
			annotations: BTreeMap::new(),
		});

		// Act
		let sa = build_service_account(&app)
			.expect("build should succeed")
			.expect("create=true → SA materialized");

		// Assert
		assert_eq!(sa.metadata.name.as_deref(), Some("custom-sa"));
	}

	#[rstest]
	fn test_build_service_account_propagates_annotations() {
		// Arrange
		let mut app = make_test_app("myapp");
		let annotations = BTreeMap::from([
			(
				"iam.gke.io/gcp-service-account".to_string(),
				"myapp@project.iam.gserviceaccount.com".to_string(),
			),
			(
				"eks.amazonaws.com/role-arn".to_string(),
				"arn:aws:iam::123456789012:role/myapp".to_string(),
			),
		]);
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: None,
			annotations: annotations.clone(),
		});

		// Act
		let sa = build_service_account(&app)
			.expect("build should succeed")
			.expect("create=true → SA materialized");

		// Assert
		let actual_annotations = sa
			.metadata
			.annotations
			.as_ref()
			.expect("annotations should propagate");
		assert_eq!(actual_annotations.len(), 2);
		assert_eq!(
			actual_annotations
				.get("iam.gke.io/gcp-service-account")
				.map(String::as_str),
			Some("myapp@project.iam.gserviceaccount.com"),
		);
		assert_eq!(
			actual_annotations
				.get("eks.amazonaws.com/role-arn")
				.map(String::as_str),
			Some("arn:aws:iam::123456789012:role/myapp"),
		);
	}

	#[rstest]
	fn test_build_service_account_owner_reference_present() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: None,
			annotations: BTreeMap::new(),
		});

		// Act
		let sa = build_service_account(&app)
			.expect("build should succeed")
			.expect("create=true → SA materialized");

		// Assert
		let owner_refs = sa
			.metadata
			.owner_references
			.as_ref()
			.expect("owner reference should be set so deletion cascades");
		assert_eq!(owner_refs.len(), 1);
		assert_eq!(owner_refs[0].kind, "Project");
		assert_eq!(owner_refs[0].name, "myapp");
	}

	#[rstest]
	fn test_build_service_account_standard_labels_present() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: None,
			annotations: BTreeMap::new(),
		});

		// Act
		let sa = build_service_account(&app)
			.expect("build should succeed")
			.expect("create=true → SA materialized");

		// Assert
		let labels = sa.metadata.labels.as_ref().expect("standard labels");
		assert_eq!(
			labels.get("app.kubernetes.io/name").map(String::as_str),
			Some("myapp")
		);
		assert_eq!(
			labels
				.get("app.kubernetes.io/managed-by")
				.map(String::as_str),
			Some("reinhardt-cloud-operator"),
		);
		assert_eq!(
			labels.get("app.kubernetes.io/instance").map(String::as_str),
			Some("myapp"),
		);
	}

	#[rstest]
	fn test_build_service_account_empty_annotations_yields_none() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: None,
			annotations: BTreeMap::new(),
		});

		// Act
		let sa = build_service_account(&app)
			.expect("build should succeed")
			.expect("create=true → SA materialized");

		// Assert
		assert!(
			sa.metadata.annotations.is_none(),
			"empty annotations BTreeMap → metadata.annotations should be None \
			 (omit the field rather than serialize an empty object)"
		);
	}

	#[rstest]
	fn test_resolved_sa_name_returns_none_when_spec_unset() {
		// Arrange
		let app = make_test_app("myapp");

		// Act / Assert
		assert_eq!(resolved_sa_name(&app), None);
	}

	#[rstest]
	fn test_resolved_sa_name_explicit_name_create_true() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: Some("foo".to_string()),
			annotations: BTreeMap::new(),
		});

		// Act / Assert
		assert_eq!(resolved_sa_name(&app), Some("foo".to_string()));
	}

	#[rstest]
	fn test_resolved_sa_name_ignores_explicit_name_create_false() {
		// Arrange — user pre-created a KSA, told us its name
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: false,
			name: Some("user-sa".to_string()),
			annotations: BTreeMap::new(),
		});

		// Act / Assert
		assert_eq!(resolved_sa_name(&app), None);
	}

	#[rstest]
	fn test_resolved_sa_name_default_name_create_true() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: None,
			annotations: BTreeMap::new(),
		});

		// Act / Assert
		assert_eq!(resolved_sa_name(&app), Some("myapp-app".to_string()));
	}

	#[rstest]
	fn test_resolved_sa_name_returns_none_when_create_false_and_no_name() {
		// Arrange — ambiguous case: user said "don't create", but didn't tell us
		// what name to bind to. The PodSpec falls back to the namespace default SA.
		let mut app = make_test_app("myapp");
		app.spec.service_account = Some(ServiceAccountSpec {
			create: false,
			name: None,
			annotations: BTreeMap::new(),
		});

		// Act / Assert
		assert_eq!(resolved_sa_name(&app), None);
	}
}
