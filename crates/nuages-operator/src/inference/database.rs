//! Database resource inference for multi-platform provisioning.
//!
//! Generates the appropriate Kubernetes resources for database provisioning
//! based on the target platform: on-premise uses StatefulSets with PVCs,
//! AWS uses ACK `DynamicObject`, and GCP uses Config Connector `DynamicObject`.
//!
//! This module is consumed by the reconciler when database provisioning is
//! integrated into the reconciliation loop (future work).
#![allow(dead_code)]

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{
	ConfigMap, Container, ContainerPort, PersistentVolumeClaim, PersistentVolumeClaimSpec, PodSpec,
	PodTemplateSpec, Secret, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use kube::ResourceExt;
use kube::api::{DynamicObject, TypeMeta};
use nuages_types::crd::{DatabaseSpec, ReinhardtApp};

use super::platform::{Platform, PlatformConfig};
use super::secrets::build_db_credentials_secret;

/// Represents a database-related Kubernetes resource produced by the
/// inference engine.
pub(crate) enum DatabaseResource {
	/// A StatefulSet for running the database engine (on-premise).
	StatefulSet(Box<StatefulSet>),
	/// A PersistentVolumeClaim for database storage (on-premise).
	Pvc(Box<PersistentVolumeClaim>),
	/// A ConfigMap for database initialisation scripts.
	ConfigMap(ConfigMap),
	/// A Secret containing database credentials.
	Secret(Secret),
	/// A dynamic object for cloud provider CRDs (ACK, Config Connector).
	Dynamic(DynamicObject),
}

/// Infer database resources based on the app spec and the target platform.
///
/// Returns an empty `Vec` when the app has no database configuration.
pub(crate) fn infer_database_resources(
	app: &ReinhardtApp,
	platform: &PlatformConfig,
) -> Vec<DatabaseResource> {
	let db = match &app.spec.database {
		Some(db) => db,
		None => return vec![],
	};

	let namespace = app.namespace().unwrap_or_default();
	let app_name = app.name_any();
	let storage_gb = db
		.storage_gb
		.unwrap_or(platform.defaults.database.storage_gb);

	match platform.platform {
		Platform::Onpremise => build_onprem_postgres(&app_name, &namespace, db, storage_gb),
		Platform::Aws => build_aws_rds(&app_name, &namespace, db),
		Platform::Gcp => build_gcp_cloud_sql(&app_name, &namespace, db, storage_gb),
	}
}

/// Build on-premise PostgreSQL resources: StatefulSet + PVC + ConfigMap + Secret.
fn build_onprem_postgres(
	app_name: &str,
	namespace: &str,
	db: &DatabaseSpec,
	storage_gb: i32,
) -> Vec<DatabaseResource> {
	let db_name = format!("{app_name}_db");
	let db_user = app_name.replace('-', "_");
	let db_password = "changeme";
	let pg_version = db.version.as_deref().unwrap_or("16");
	let labels = standard_db_labels(app_name);

	// StatefulSet running postgres
	let stateful_set = StatefulSet {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-db")),
			namespace: Some(namespace.to_string()),
			labels: Some(labels.clone()),
			..Default::default()
		},
		spec: Some(StatefulSetSpec {
			replicas: Some(1),
			selector: LabelSelector {
				match_labels: Some(BTreeMap::from([(
					"app.kubernetes.io/name".to_string(),
					format!("{app_name}-db"),
				)])),
				..Default::default()
			},
			template: PodTemplateSpec {
				metadata: Some(ObjectMeta {
					labels: Some(BTreeMap::from([(
						"app.kubernetes.io/name".to_string(),
						format!("{app_name}-db"),
					)])),
					..Default::default()
				}),
				spec: Some(PodSpec {
					containers: vec![Container {
						name: "postgres".to_string(),
						image: Some(format!("postgres:{pg_version}")),
						ports: Some(vec![ContainerPort {
							container_port: 5432,
							name: Some("postgres".to_string()),
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
			..Default::default()
		}),
		..Default::default()
	};

	// PVC for data storage
	let pvc = PersistentVolumeClaim {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-db-data")),
			namespace: Some(namespace.to_string()),
			labels: Some(labels.clone()),
			..Default::default()
		},
		spec: Some(PersistentVolumeClaimSpec {
			access_modes: Some(vec!["ReadWriteOnce".to_string()]),
			resources: Some(k8s_openapi::api::core::v1::VolumeResourceRequirements {
				requests: Some(BTreeMap::from([(
					"storage".to_string(),
					Quantity(format!("{storage_gb}Gi")),
				)])),
				..Default::default()
			}),
			..Default::default()
		}),
		..Default::default()
	};

	// Init ConfigMap with database creation SQL
	let init_sql = format!(
		"CREATE DATABASE {db_name};\n\
		 CREATE USER {db_user} WITH PASSWORD '{db_password}';\n\
		 GRANT ALL PRIVILEGES ON DATABASE {db_name} TO {db_user};\n"
	);
	let config_map = ConfigMap {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-db-init")),
			namespace: Some(namespace.to_string()),
			labels: Some(labels),
			..Default::default()
		},
		data: Some(BTreeMap::from([("init.sql".to_string(), init_sql)])),
		..Default::default()
	};

	// Credentials secret
	let secret = build_db_credentials_secret(app_name, namespace, &db_user, db_password);

	vec![
		DatabaseResource::StatefulSet(Box::new(stateful_set)),
		DatabaseResource::Pvc(Box::new(pvc)),
		DatabaseResource::ConfigMap(config_map),
		DatabaseResource::Secret(secret),
	]
}

/// Build AWS RDS resources via ACK (stub: returns DynamicObject for DBInstance + Secret).
fn build_aws_rds(app_name: &str, namespace: &str, _db: &DatabaseSpec) -> Vec<DatabaseResource> {
	let db_instance = DynamicObject {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-rds")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_db_labels(app_name)),
			..Default::default()
		},
		types: Some(TypeMeta {
			api_version: "rds.services.k8s.aws/v1alpha1".to_string(),
			kind: "DBInstance".to_string(),
		}),
		data: serde_json::json!({}),
	};

	let secret = build_db_credentials_secret(app_name, namespace, app_name, "changeme");

	vec![
		DatabaseResource::Dynamic(db_instance),
		DatabaseResource::Secret(secret),
	]
}

/// Build GCP Cloud SQL resources via Config Connector
/// (stub: returns DynamicObjects for SQLInstance, SQLDatabase, SQLUser + Secret).
fn build_gcp_cloud_sql(
	app_name: &str,
	namespace: &str,
	_db: &DatabaseSpec,
	_storage_gb: i32,
) -> Vec<DatabaseResource> {
	let sql_instance = DynamicObject {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-sql-instance")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_db_labels(app_name)),
			..Default::default()
		},
		types: Some(TypeMeta {
			api_version: "sql.cnrm.cloud.google.com/v1beta1".to_string(),
			kind: "SQLInstance".to_string(),
		}),
		data: serde_json::json!({}),
	};

	let sql_database = DynamicObject {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-sql-database")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_db_labels(app_name)),
			..Default::default()
		},
		types: Some(TypeMeta {
			api_version: "sql.cnrm.cloud.google.com/v1beta1".to_string(),
			kind: "SQLDatabase".to_string(),
		}),
		data: serde_json::json!({}),
	};

	let sql_user = DynamicObject {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-sql-user")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_db_labels(app_name)),
			..Default::default()
		},
		types: Some(TypeMeta {
			api_version: "sql.cnrm.cloud.google.com/v1beta1".to_string(),
			kind: "SQLUser".to_string(),
		}),
		data: serde_json::json!({}),
	};

	let secret = build_db_credentials_secret(app_name, namespace, app_name, "changeme");

	vec![
		DatabaseResource::Dynamic(sql_instance),
		DatabaseResource::Dynamic(sql_database),
		DatabaseResource::Dynamic(sql_user),
		DatabaseResource::Secret(secret),
	]
}

fn standard_db_labels(app_name: &str) -> BTreeMap<String, String> {
	BTreeMap::from([
		("app.kubernetes.io/name".to_string(), app_name.to_string()),
		(
			"app.kubernetes.io/managed-by".to_string(),
			"nuages-operator".to_string(),
		),
		(
			"app.kubernetes.io/component".to_string(),
			"database".to_string(),
		),
	])
}

#[cfg(test)]
mod tests {
	use super::*;
	use nuages_types::crd::policy::DeletionPolicy;
	use nuages_types::crd::{DatabaseEngine, ReinhardtAppSpec};
	use rstest::rstest;

	fn make_app_with_db(name: &str, db_spec: DatabaseSpec) -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "myapp:latest".to_string(),
				replicas: None,
				database: Some(db_spec),
				cache: None,
				worker: None,
				auth: None,
				storage: None,
				mail: None,
				scale: None,
				health: None,
				services: None,
				deletion_policy: DeletionPolicy::default(),
				features: vec![],
				env: BTreeMap::new(),
			},
			status: None,
		}
	}

	fn make_app_without_db(name: &str) -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "myapp:latest".to_string(),
				replicas: None,
				database: None,
				cache: None,
				worker: None,
				auth: None,
				storage: None,
				mail: None,
				scale: None,
				health: None,
				services: None,
				deletion_policy: DeletionPolicy::default(),
				features: vec![],
				env: BTreeMap::new(),
			},
			status: None,
		}
	}

	#[rstest]
	fn no_database_spec_returns_empty() {
		// Arrange
		let app = make_app_without_db("myapp");
		let platform = PlatformConfig::onprem_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		assert!(resources.is_empty());
	}

	#[rstest]
	fn onprem_generates_four_resources() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(20),
			version: Some("16".to_string()),
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::onprem_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		assert_eq!(resources.len(), 4);
		assert!(matches!(&resources[0], DatabaseResource::StatefulSet(..)));
		assert!(matches!(&resources[1], DatabaseResource::Pvc(..)));
		assert!(matches!(&resources[2], DatabaseResource::ConfigMap(_)));
		assert!(matches!(&resources[3], DatabaseResource::Secret(_)));
	}

	#[rstest]
	fn onprem_statefulset_uses_correct_image() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: None,
			version: Some("15".to_string()),
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::onprem_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::StatefulSet(ss) = &resources[0] {
			let container = &ss
				.spec
				.as_ref()
				.unwrap()
				.template
				.spec
				.as_ref()
				.unwrap()
				.containers[0];
			assert_eq!(container.image.as_deref(), Some("postgres:15"));
		} else {
			panic!("Expected StatefulSet as first resource");
		}
	}

	#[rstest]
	fn onprem_pvc_uses_correct_storage_size() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(50),
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::onprem_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Pvc(pvc) = &resources[1] {
			let requests = pvc
				.spec
				.as_ref()
				.unwrap()
				.resources
				.as_ref()
				.unwrap()
				.requests
				.as_ref()
				.unwrap();
			assert_eq!(requests["storage"].0, "50Gi");
		} else {
			panic!("Expected PVC as second resource");
		}
	}

	#[rstest]
	fn onprem_uses_platform_default_storage_when_unset() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::onprem_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Pvc(pvc) = &resources[1] {
			let requests = pvc
				.spec
				.as_ref()
				.unwrap()
				.resources
				.as_ref()
				.unwrap()
				.requests
				.as_ref()
				.unwrap();
			assert_eq!(requests["storage"].0, "20Gi");
		} else {
			panic!("Expected PVC as second resource");
		}
	}

	#[rstest]
	fn aws_generates_two_resources() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::aws_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		assert_eq!(resources.len(), 2);
		assert!(matches!(&resources[0], DatabaseResource::Dynamic(_)));
		assert!(matches!(&resources[1], DatabaseResource::Secret(_)));
	}

	#[rstest]
	fn aws_dynamic_object_has_rds_type() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::aws_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let types = obj.types.as_ref().unwrap();
			assert_eq!(types.kind, "DBInstance");
			assert_eq!(types.api_version, "rds.services.k8s.aws/v1alpha1");
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn gcp_generates_four_resources() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::gcp_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		assert_eq!(resources.len(), 4);
		assert!(matches!(&resources[0], DatabaseResource::Dynamic(_)));
		assert!(matches!(&resources[1], DatabaseResource::Dynamic(_)));
		assert!(matches!(&resources[2], DatabaseResource::Dynamic(_)));
		assert!(matches!(&resources[3], DatabaseResource::Secret(_)));
	}

	#[rstest]
	fn gcp_dynamic_objects_have_config_connector_types() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::gcp_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let types = obj.types.as_ref().unwrap();
			assert_eq!(types.kind, "SQLInstance");
		} else {
			panic!("Expected Dynamic as first resource");
		}

		if let DatabaseResource::Dynamic(obj) = &resources[1] {
			let types = obj.types.as_ref().unwrap();
			assert_eq!(types.kind, "SQLDatabase");
		} else {
			panic!("Expected Dynamic as second resource");
		}

		if let DatabaseResource::Dynamic(obj) = &resources[2] {
			let types = obj.types.as_ref().unwrap();
			assert_eq!(types.kind, "SQLUser");
		} else {
			panic!("Expected Dynamic as third resource");
		}
	}

	#[rstest]
	fn onprem_custom_storage_gb_overrides_platform_default() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(100),
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::onprem_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Pvc(pvc) = &resources[1] {
			let requests = pvc
				.spec
				.as_ref()
				.unwrap()
				.resources
				.as_ref()
				.unwrap()
				.requests
				.as_ref()
				.unwrap();
			assert_eq!(requests["storage"].0, "100Gi");
		} else {
			panic!("Expected PVC as second resource");
		}
	}

	#[rstest]
	fn onprem_default_pg_version_is_16() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::onprem_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::StatefulSet(ss) = &resources[0] {
			let container = &ss
				.spec
				.as_ref()
				.unwrap()
				.template
				.spec
				.as_ref()
				.unwrap()
				.containers[0];
			assert_eq!(container.image.as_deref(), Some("postgres:16"));
		} else {
			panic!("Expected StatefulSet as first resource");
		}
	}
}
