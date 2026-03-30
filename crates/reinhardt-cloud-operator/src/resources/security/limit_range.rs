//! LimitRange builders for noisy neighbor protection.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{LimitRange, LimitRangeItem, LimitRangeSpec};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::ReinhardtApp;

use crate::error::Error;
use crate::inference::platform::ResourceDefaults;
use crate::resources::labels::owner_reference;

/// Builds a `LimitRange` that sets default resource requests/limits
/// for containers in the app's namespace.
pub(crate) fn build_limit_range(
	app: &ReinhardtApp,
	defaults: &ResourceDefaults,
) -> Result<LimitRange, Error> {
	let name = format!("{}-limits", app.name_any());
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;

	Ok(LimitRange {
		metadata: ObjectMeta {
			name: Some(name),
			namespace: Some(namespace),
			owner_references: Some(vec![owner_ref]),
			labels: Some(BTreeMap::from([(
				"app.kubernetes.io/managed-by".to_string(),
				"reinhardt-cloud-operator".to_string(),
			)])),
			..Default::default()
		},
		spec: Some(LimitRangeSpec {
			limits: vec![LimitRangeItem {
				type_: "Container".to_string(),
				default: Some(BTreeMap::from([
					("cpu".to_string(), Quantity(defaults.cpu_limit.clone())),
					(
						"memory".to_string(),
						Quantity(defaults.memory_limit.clone()),
					),
				])),
				default_request: Some(BTreeMap::from([
					("cpu".to_string(), Quantity(defaults.cpu_request.clone())),
					(
						"memory".to_string(),
						Quantity(defaults.memory_request.clone()),
					),
				])),
				..Default::default()
			}],
		}),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ReinhardtAppSpec;
	use rstest::rstest;

	fn test_app() -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some("myapp".to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "test:latest".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	fn test_defaults() -> ResourceDefaults {
		ResourceDefaults {
			cpu_request: "100m".to_string(),
			memory_request: "128Mi".to_string(),
			cpu_limit: "1000m".to_string(),
			memory_limit: "1Gi".to_string(),
		}
	}

	#[rstest]
	fn limit_range_has_correct_name_and_namespace() {
		// Arrange
		let app = test_app();

		// Act
		let lr = build_limit_range(&app, &test_defaults()).unwrap();

		// Assert
		assert_eq!(lr.metadata.name.as_deref(), Some("myapp-limits"));
		assert_eq!(lr.metadata.namespace.as_deref(), Some("default"));
	}

	#[rstest]
	fn limit_range_has_owner_reference() {
		// Arrange
		let app = test_app();

		// Act
		let lr = build_limit_range(&app, &test_defaults()).unwrap();

		// Assert
		let owner_refs = lr.metadata.owner_references.unwrap();
		assert_eq!(owner_refs.len(), 1);
		assert_eq!(owner_refs[0].name, "myapp");
	}

	#[rstest]
	fn limit_range_sets_container_defaults() {
		// Arrange
		let app = test_app();

		// Act
		let lr = build_limit_range(&app, &test_defaults()).unwrap();

		// Assert
		let limits = lr.spec.unwrap().limits;
		assert_eq!(limits.len(), 1);
		assert_eq!(limits[0].type_, "Container");
		let default_req = limits[0].default_request.as_ref().unwrap();
		assert!(default_req.contains_key("cpu"));
		assert!(default_req.contains_key("memory"));
	}
}
