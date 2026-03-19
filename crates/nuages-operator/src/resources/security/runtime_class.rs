//! RuntimeClass resolution for workload isolation.
//!
//! Maps the CRD's `IsolationLevel` to a Kubernetes `RuntimeClass` name
//! based on the target platform.

use crate::inference::platform::Platform;
use nuages_types::crd::isolation::IsolationLevel;
use nuages_types::crd::ReinhardtApp;

/// Resolves the RuntimeClass name based on isolation level and platform.
///
/// Returns `None` when no isolation is configured (standard runc runtime).
/// The `runtime_class_override` field takes precedence when set.
pub(crate) fn resolve_runtime_class_name(
	app: &ReinhardtApp,
	platform: &Platform,
) -> Option<String> {
	let isolation = app.spec.isolation.as_ref()?;

	if let Some(ref name) = isolation.runtime_class_override {
		return Some(name.clone());
	}

	match isolation.level {
		IsolationLevel::None => None,
		IsolationLevel::Sandbox => match platform {
			Platform::Aws | Platform::Gcp | Platform::Onpremise => {
				Some("gvisor".to_string())
			}
		},
		IsolationLevel::MicroVM => Some("kata-clh".to_string()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use nuages_types::crd::isolation::IsolationSpec;
	use rstest::rstest;

	#[rstest]
	#[case(Platform::Aws, IsolationLevel::None, None)]
	#[case(Platform::Gcp, IsolationLevel::None, None)]
	#[case(Platform::Onpremise, IsolationLevel::None, None)]
	#[case(Platform::Aws, IsolationLevel::Sandbox, Some("gvisor".to_string()))]
	#[case(Platform::Gcp, IsolationLevel::Sandbox, Some("gvisor".to_string()))]
	#[case(Platform::Onpremise, IsolationLevel::Sandbox, Some("gvisor".to_string()))]
	#[case(Platform::Aws, IsolationLevel::MicroVM, Some("kata-clh".to_string()))]
	#[case(Platform::Gcp, IsolationLevel::MicroVM, Some("kata-clh".to_string()))]
	#[case(Platform::Onpremise, IsolationLevel::MicroVM, Some("kata-clh".to_string()))]
	fn resolve_runtime_class_maps_correctly(
		#[case] platform: Platform,
		#[case] level: IsolationLevel,
		#[case] expected: Option<String>,
	) {
		// Arrange
		let app = test_app_with_isolation(Some(IsolationSpec {
			level,
			..Default::default()
		}));

		// Act
		let result = resolve_runtime_class_name(&app, &platform);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	fn resolve_runtime_class_returns_none_without_isolation() {
		// Arrange
		let app = test_app_with_isolation(None);

		// Act
		let result = resolve_runtime_class_name(&app, &Platform::Aws);

		// Assert
		assert_eq!(result, None);
	}

	#[rstest]
	fn resolve_runtime_class_override_takes_precedence() {
		// Arrange
		let app = test_app_with_isolation(Some(IsolationSpec {
			level: IsolationLevel::MicroVM,
			runtime_class_override: Some("kata-fc".to_string()),
			..Default::default()
		}));

		// Act
		let result = resolve_runtime_class_name(&app, &Platform::Aws);

		// Assert
		assert_eq!(result, Some("kata-fc".to_string()));
	}

	fn test_app_with_isolation(isolation: Option<IsolationSpec>) -> ReinhardtApp {
		use kube::api::ObjectMeta;
		use nuages_types::crd::ReinhardtAppSpec;
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some("test-app".to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "test:latest".to_string(),
				isolation,
				..Default::default()
			},
			status: None,
		}
	}
}
