//! PostgreSQL resource builders for operator-managed database instances.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{
	Container, ContainerPort, EnvFromSource, PersistentVolumeClaim, PersistentVolumeClaimSpec,
	PodSpec, PodTemplateSpec, Secret, SecretEnvSource, Service, ServicePort, ServiceSpec,
	VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use kube::ResourceExt;
use rand::Rng;
use reinhardt_cloud_types::crd::Project;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Generates a random 32-character alphanumeric password.
fn generate_password() -> String {
	const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
	let mut rng = rand::rng();
	(0..32)
		.map(|_| {
			let idx = rng.random_range(0..CHARSET.len());
			CHARSET[idx] as char
		})
		.collect()
}

/// Sanitizes an project name for use as a PostgreSQL database/user name.
///
/// Replaces hyphens with underscores since PostgreSQL identifiers do not allow hyphens.
/// Strips non-alphanumeric/underscore characters, prefixes with underscore if the name
/// starts with a digit, and truncates to 63 characters (PostgreSQL identifier limit).
fn sanitize_db_name(name: &str) -> String {
	let mut sanitized: String = name
		.replace('-', "_")
		.chars()
		.filter(|c| c.is_ascii_alphanumeric() || *c == '_')
		.collect();

	// PostgreSQL identifiers must not start with a digit
	if sanitized.starts_with(|c: char| c.is_ascii_digit()) {
		sanitized.insert(0, '_');
	}

	// PostgreSQL identifier length limit
	sanitized.truncate(63);
	sanitized
}

/// Builds a `Secret` containing PostgreSQL credentials for the given `Project`.
///
/// The secret includes `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB`,
/// and a fully-formed `DATABASE_URL` connection string.
pub(crate) fn build_db_secret(app: &Project) -> Result<Secret, Error> {
	let labels = standard_labels(app, Component::Database);
	let namespace = super::require_namespace(app)?;
	let owner_ref = owner_reference(app)?;
	let project_name = app.name_any();

	let user = sanitize_db_name(&project_name);
	let password = generate_password();
	let db = user.clone();
	let database_url = format!(
		"postgresql://{}:{}@{}-postgresql:5432/{}",
		user, password, project_name, db
	);

	Ok(Secret {
		metadata: ObjectMeta {
			name: Some(format!("{}-db-credentials", project_name)),
			namespace: Some(namespace),
			labels: Some(labels),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		type_: Some("Opaque".to_string()),
		string_data: Some(BTreeMap::from([
			("POSTGRES_USER".to_string(), user),
			("POSTGRES_PASSWORD".to_string(), password),
			("POSTGRES_DB".to_string(), db),
			("DATABASE_URL".to_string(), database_url),
		])),
		..Default::default()
	})
}

/// Builds a `StatefulSet` running PostgreSQL for the given `Project`.
///
/// Uses `postgres:16-alpine` with a 1Gi PVC mounted at `/var/lib/postgresql/data`.
/// Credentials are injected from the companion secret via `envFrom`.
pub(crate) fn build_db_statefulset(app: &Project) -> Result<StatefulSet, Error> {
	let labels = standard_labels(app, Component::Database);
	let namespace = super::require_namespace(app)?;
	let owner_ref = owner_reference(app)?;
	let project_name = app.name_any();
	let sts_name = format!("{}-postgresql", project_name);
	let secret_name = format!("{}-db-credentials", project_name);

	Ok(StatefulSet {
		metadata: ObjectMeta {
			name: Some(sts_name.clone()),
			namespace: Some(namespace),
			labels: Some(labels.clone()),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(StatefulSetSpec {
			replicas: Some(1),
			service_name: Some(sts_name),
			selector: LabelSelector {
				match_labels: Some(BTreeMap::from([
					("app.kubernetes.io/name".to_string(), project_name.clone()),
					(
						"app.kubernetes.io/component".to_string(),
						"database".to_string(),
					),
				])),
				..Default::default()
			},
			template: PodTemplateSpec {
				metadata: Some(ObjectMeta {
					labels: Some(labels),
					..Default::default()
				}),
				spec: Some(PodSpec {
					containers: vec![Container {
						name: "postgresql".to_string(),
						image: Some("postgres:16-alpine".to_string()),
						ports: Some(vec![ContainerPort {
							container_port: 5432,
							name: Some("postgresql".to_string()),
							..Default::default()
						}]),
						env_from: Some(vec![EnvFromSource {
							secret_ref: Some(SecretEnvSource {
								name: secret_name,
								..Default::default()
							}),
							..Default::default()
						}]),
						volume_mounts: Some(vec![VolumeMount {
							name: "data".to_string(),
							mount_path: "/var/lib/postgresql/data".to_string(),
							..Default::default()
						}]),
						..Default::default()
					}],
					..Default::default()
				}),
			},
			volume_claim_templates: Some(vec![PersistentVolumeClaim {
				metadata: ObjectMeta {
					name: Some("data".to_string()),
					..Default::default()
				},
				spec: Some(PersistentVolumeClaimSpec {
					access_modes: Some(vec!["ReadWriteOnce".to_string()]),
					resources: Some(k8s_openapi::api::core::v1::VolumeResourceRequirements {
						requests: Some(BTreeMap::from([(
							"storage".to_string(),
							Quantity("1Gi".to_string()),
						)])),
						..Default::default()
					}),
					..Default::default()
				}),
				..Default::default()
			}]),
			..Default::default()
		}),
		..Default::default()
	})
}

/// Builds a headless-style `Service` exposing PostgreSQL for the given `Project`.
///
/// Targets port 5432 and selects pods by app name and database component labels.
pub(crate) fn build_db_service(app: &Project) -> Result<Service, Error> {
	let labels = standard_labels(app, Component::Database);
	let namespace = super::require_namespace(app)?;
	let owner_ref = owner_reference(app)?;
	let project_name = app.name_any();

	Ok(Service {
		metadata: ObjectMeta {
			name: Some(format!("{}-postgresql", project_name)),
			namespace: Some(namespace),
			labels: Some(labels),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(ServiceSpec {
			type_: Some("ClusterIP".to_string()),
			selector: Some(BTreeMap::from([
				("app.kubernetes.io/name".to_string(), project_name),
				(
					"app.kubernetes.io/component".to_string(),
					"database".to_string(),
				),
			])),
			ports: Some(vec![ServicePort {
				port: 5432,
				target_port: Some(
					k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(5432),
				),
				name: Some("postgresql".to_string()),
				..Default::default()
			}]),
			..Default::default()
		}),
		..Default::default()
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ProjectSpec;
	use rstest::rstest;

	fn test_app(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
				image: "myapp:v1".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_build_db_secret_has_correct_name() {
		// Arrange
		let app = test_app("my-app");

		// Act
		let secret = build_db_secret(&app).expect("build should succeed");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("my-app-db-credentials")
		);
	}

	#[rstest]
	fn test_build_db_secret_contains_required_keys() {
		// Arrange
		let app = test_app("my-app");

		// Act
		let secret = build_db_secret(&app).expect("build should succeed");
		let data = secret.string_data.as_ref().unwrap();

		// Assert
		assert!(data.contains_key("POSTGRES_USER"));
		assert!(data.contains_key("POSTGRES_PASSWORD"));
		assert!(data.contains_key("POSTGRES_DB"));
		assert!(data.contains_key("DATABASE_URL"));
	}

	#[rstest]
	fn test_build_db_secret_password_length() {
		// Arrange
		let app = test_app("my-app");

		// Act
		let secret = build_db_secret(&app).expect("build should succeed");
		let data = secret.string_data.as_ref().unwrap();
		let password = data.get("POSTGRES_PASSWORD").unwrap();

		// Assert
		assert_eq!(password.len(), 32);
		assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
	}

	#[rstest]
	fn test_build_db_secret_sanitizes_name() {
		// Arrange
		let app = test_app("my-app");

		// Act
		let secret = build_db_secret(&app).expect("build should succeed");
		let data = secret.string_data.as_ref().unwrap();

		// Assert
		assert_eq!(data.get("POSTGRES_USER").unwrap(), "my_app");
		assert_eq!(data.get("POSTGRES_DB").unwrap(), "my_app");
	}

	#[rstest]
	fn test_build_db_statefulset_name() {
		// Arrange
		let app = test_app("my-app");

		// Act
		let sts = build_db_statefulset(&app).expect("build should succeed");

		// Assert
		assert_eq!(sts.metadata.name.as_deref(), Some("my-app-postgresql"));
	}

	#[rstest]
	fn test_build_db_statefulset_container_image() {
		// Arrange
		let app = test_app("my-app");

		// Act
		let sts = build_db_statefulset(&app).expect("build should succeed");
		let containers = &sts.spec.unwrap().template.spec.unwrap().containers;

		// Assert
		assert_eq!(containers[0].image.as_deref(), Some("postgres:16-alpine"));
	}

	#[rstest]
	fn test_build_db_statefulset_has_pvc() {
		// Arrange
		let app = test_app("my-app");

		// Act
		let sts = build_db_statefulset(&app).expect("build should succeed");
		let vcts = sts.spec.unwrap().volume_claim_templates.unwrap();

		// Assert
		assert_eq!(vcts.len(), 1);
		assert_eq!(vcts[0].metadata.name.as_deref(), Some("data"));
		let resources = vcts[0].spec.as_ref().unwrap().resources.as_ref().unwrap();
		let storage = resources.requests.as_ref().unwrap().get("storage").unwrap();
		assert_eq!(storage.0, "1Gi");
	}

	#[rstest]
	fn test_build_db_service_port() {
		// Arrange
		let app = test_app("my-app");

		// Act
		let svc = build_db_service(&app).expect("build should succeed");
		let ports = svc.spec.unwrap().ports.unwrap();

		// Assert
		assert_eq!(ports.len(), 1);
		assert_eq!(ports[0].port, 5432);
		assert_eq!(
			ports[0].target_port,
			Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(5432))
		);
	}
}
