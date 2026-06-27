//! Kubernetes resource builders for operator-managed resources.

pub(crate) mod autoscaler;
pub(crate) mod cache;
pub(crate) mod credentials;
pub(crate) mod database;
pub(crate) mod deployment;
pub(crate) mod grpc;
pub(crate) mod i18n;
pub(crate) mod ingress;
pub(crate) mod issuer;
pub(crate) mod labels;
pub(crate) mod mail;
pub(crate) mod migration;
pub(crate) mod plugins;
pub(crate) mod preview;
pub(crate) mod preview_namespace;
pub(crate) mod preview_status;
pub(crate) mod security;
pub(crate) mod service;
pub(crate) mod service_account;
pub(crate) mod source;
pub(crate) mod storage;
pub(crate) mod tenant;
pub(crate) mod worker;

// Re-exports for convenient access from parent modules
pub(crate) use autoscaler::{AutoscalerPlan, build_autoscaler, hpa_is_ready};
pub(crate) use database::{build_db_secret, build_db_service, build_db_statefulset};
pub(crate) use deployment::build_deployment;
pub(crate) use ingress::build_ingress;
pub(crate) use service::build_service;
// source::build_kaniko_job and source::should_build_from_source are used
// directly via crate::resources::source in the reconciler.

/// Extracts the namespace from a `Project`, returning
/// `Error::MissingNamespace` if absent.
pub(crate) fn require_namespace(
	app: &reinhardt_cloud_types::crd::Project,
) -> Result<String, crate::error::Error> {
	use kube::ResourceExt;
	let name = app.name_any();
	app.namespace()
		.ok_or(crate::error::Error::MissingNamespace(name))
}

/// Validates that a port number is within the valid TCP/UDP range (1-65535).
pub(crate) fn validate_port(field: &'static str, port: i32) -> Result<i32, crate::error::Error> {
	if (1..=65535).contains(&port) {
		Ok(port)
	} else {
		Err(crate::error::Error::InvalidPort { field, port })
	}
}

/// Returns image-pull secrets after enforcing app-owned secret names.
///
/// `spec.imagePullSecrets` is user-controlled. Restricting names to the
/// application-owned prefix prevents a `Project` author from causing kubelet
/// to use shared namespace registry credentials that the author cannot read.
/// Operator-created preview environments may also use the parent app's prefix
/// when their metadata matches the preview namespace and label contract.
pub(crate) fn validated_image_pull_secrets(
	app: &reinhardt_cloud_types::crd::Project,
) -> Result<Option<Vec<k8s_openapi::api::core::v1::LocalObjectReference>>, crate::error::Error> {
	let app_name = {
		use kube::ResourceExt;
		app.name_any()
	};
	let Some(secrets) = app.spec.image_pull_secrets.as_ref() else {
		return Ok(None);
	};
	let allowed_prefixes = image_pull_secret_prefixes(app, &app_name);

	for secret in secrets {
		if !allowed_prefixes
			.iter()
			.any(|prefix| secret.name.starts_with(prefix))
		{
			return Err(crate::error::Error::InvalidImagePullSecret {
				app: app_name,
				secret: secret.name.clone(),
				allowed_prefixes: allowed_prefixes.join(", "),
			});
		}
	}

	Ok(Some(secrets.clone()))
}

fn image_pull_secret_prefixes(
	app: &reinhardt_cloud_types::crd::Project,
	app_name: &str,
) -> Vec<String> {
	let app_prefix = format!("{app_name}-");
	let Some(parent_name) = verified_preview_parent_name(app, app_name) else {
		return vec![app_prefix];
	};

	let parent_prefix = format!("{parent_name}-");
	if parent_prefix == app_prefix {
		vec![app_prefix]
	} else {
		vec![app_prefix, parent_prefix]
	}
}

fn verified_preview_parent_name(
	app: &reinhardt_cloud_types::crd::Project,
	app_name: &str,
) -> Option<String> {
	let labels = app.metadata.labels.as_ref()?;
	let parent_name = labels.get(crate::resources::preview::PARENT_APP_LABEL_KEY)?;
	let pr_number = labels.get(crate::resources::preview::PR_NUMBER_LABEL_KEY)?;

	if labels
		.get(crate::resources::preview::PREVIEW_LABEL_KEY)
		.map(String::as_str)
		!= Some("true")
		|| labels
			.get("app.kubernetes.io/managed-by")
			.map(String::as_str)
			!= Some(crate::resources::preview::MANAGED_BY_LABEL_VALUE)
		|| !pr_number
			.chars()
			.all(|character| character.is_ascii_digit())
		|| pr_number.is_empty()
	{
		return None;
	}

	let expected_name = crate::resources::preview::preview_project_name(parent_name, pr_number);
	if app_name == expected_name && verified_preview_namespace(app, labels, parent_name) {
		Some(parent_name.clone())
	} else {
		None
	}
}

fn verified_preview_namespace(
	app: &reinhardt_cloud_types::crd::Project,
	labels: &std::collections::BTreeMap<String, String>,
	parent_name: &str,
) -> bool {
	let Some(namespace) = app.metadata.namespace.as_deref() else {
		return false;
	};
	let Some(parent_namespace) = labels.get(crate::resources::preview::PARENT_NAMESPACE_LABEL_KEY)
	else {
		return crate::resources::preview_namespace::legacy_preview_namespace_matches(
			namespace,
			parent_name,
		);
	};
	let expected_namespace =
		crate::resources::preview_namespace::preview_namespace_name(parent_namespace, parent_name);
	namespace == expected_namespace
}

#[cfg(test)]
mod tests {
	use super::*;
	use k8s_openapi::api::core::v1::LocalObjectReference;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::{Project, ProjectSpec};
	use rstest::rstest;

	fn test_app(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
				image: "example/app:latest".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_validate_port_accepts_boundary_values() {
		// Arrange / Act / Assert
		assert_eq!(validate_port("port", 1).unwrap(), 1);
		assert_eq!(validate_port("port", 65535).unwrap(), 65535);
	}

	#[rstest]
	fn test_validated_image_pull_secrets_accepts_app_owned_names() {
		// Arrange
		let mut app = test_app("web");
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "web-regcred".to_string(),
		}]);

		// Act
		let secrets = validated_image_pull_secrets(&app)
			.expect("app-owned image pull secret should be accepted")
			.expect("image pull secrets should be present");

		// Assert
		assert_eq!(secrets.len(), 1);
		assert_eq!(secrets[0].name, "web-regcred");
	}

	#[rstest]
	fn test_validated_image_pull_secrets_accepts_preview_parent_owned_names() {
		// Arrange
		let mut app = test_app("web-pr-42");
		app.metadata.namespace = Some(crate::resources::preview_namespace::preview_namespace_name(
			"default", "web",
		));
		app.metadata.labels = Some(crate::resources::preview::preview_labels(
			"default", "web", "42",
		));
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "web-regcred".to_string(),
		}]);

		// Act
		let secrets = validated_image_pull_secrets(&app)
			.expect("parent-owned image pull secret should be accepted for previews")
			.expect("image pull secrets should be present");

		// Assert
		assert_eq!(secrets.len(), 1);
		assert_eq!(secrets[0].name, "web-regcred");
	}

	#[rstest]
	fn test_validated_image_pull_secrets_accepts_legacy_preview_parent_owned_names() {
		// Arrange
		let mut app = test_app("web-pr-42");
		app.metadata.namespace = Some(crate::resources::preview_namespace::preview_namespace_name(
			"default", "web",
		));
		let mut labels = crate::resources::preview::preview_labels("default", "web", "42");
		labels.remove(crate::resources::preview::PARENT_NAMESPACE_LABEL_KEY);
		app.metadata.labels = Some(labels);
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "web-regcred".to_string(),
		}]);

		// Act
		let secrets = validated_image_pull_secrets(&app)
			.expect("legacy preview image pull secret should be accepted")
			.expect("image pull secrets should be present");

		// Assert
		assert_eq!(secrets.len(), 1);
		assert_eq!(secrets[0].name, "web-regcred");
	}

	#[rstest]
	fn test_validated_image_pull_secrets_rejects_legacy_preview_truncated_namespace() {
		// Arrange
		let parent_namespace =
			"tenant-with-a-very-long-namespace-name-that-truncates-the-preview-prefix";
		let mut app = test_app("web-pr-42");
		app.metadata.namespace = Some(crate::resources::preview_namespace::preview_namespace_name(
			parent_namespace,
			"web",
		));
		let mut labels = crate::resources::preview::preview_labels(parent_namespace, "web", "42");
		labels.remove(crate::resources::preview::PARENT_NAMESPACE_LABEL_KEY);
		app.metadata.labels = Some(labels);
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "web-regcred".to_string(),
		}]);

		// Act
		let error = validated_image_pull_secrets(&app)
			.expect_err("legacy preview with a truncated namespace should be rejected");

		// Assert
		assert!(matches!(
			error,
			crate::error::Error::InvalidImagePullSecret { .. }
		));
	}

	#[rstest]
	fn test_validated_image_pull_secrets_rejects_legacy_preview_wrong_namespace() {
		// Arrange
		let mut app = test_app("web-pr-42");
		app.metadata.namespace = Some("default".to_string());
		let mut labels = crate::resources::preview::preview_labels("default", "web", "42");
		labels.remove(crate::resources::preview::PARENT_NAMESPACE_LABEL_KEY);
		app.metadata.labels = Some(labels);
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "web-regcred".to_string(),
		}]);

		// Act
		let error = validated_image_pull_secrets(&app)
			.expect_err("legacy preview in a non-canonical namespace should be rejected");

		// Assert
		assert!(matches!(
			error,
			crate::error::Error::InvalidImagePullSecret { .. }
		));
	}

	#[rstest]
	fn test_validated_image_pull_secrets_rejects_spoofed_preview_parent_owned_names() {
		// Arrange
		let mut app = test_app("web-pr-evil");
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "web-regcred".to_string(),
		}]);

		// Act
		let error = validated_image_pull_secrets(&app)
			.expect_err("spoofed preview image pull secret should be rejected");

		// Assert
		assert!(matches!(
			error,
			crate::error::Error::InvalidImagePullSecret { .. }
		));
	}

	#[rstest]
	fn test_validated_image_pull_secrets_rejects_shared_secret_names() {
		// Arrange
		let mut app = test_app("web");
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "platform-regcred".to_string(),
		}]);

		// Act
		let error = validated_image_pull_secrets(&app)
			.expect_err("shared image pull secret should be rejected");

		// Assert
		assert!(matches!(
			error,
			crate::error::Error::InvalidImagePullSecret { .. }
		));
	}
}
