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
use reinhardt_cloud_types::crd::{DatabaseEngine, DatabaseSpec, ReinhardtApp};

use super::platform::{Platform, PlatformConfig};
use super::secrets::{build_db_credentials_secret, generate_random_password};

/// Map a `DatabaseEngine` variant to the lowercase engine name used by AWS RDS.
fn engine_to_string(engine: &DatabaseEngine) -> &str {
	match engine {
		DatabaseEngine::Postgresql => "postgres",
		DatabaseEngine::Mysql => "mysql",
		DatabaseEngine::Sqlite => "sqlite",
	}
}

/// Build a GCP Cloud SQL `databaseVersion` string from engine and optional
/// version (e.g. `POSTGRES_16`, `MYSQL_8_0`).
fn engine_to_gcp_version(engine: &DatabaseEngine, version: Option<&str>) -> String {
	let prefix = match engine {
		DatabaseEngine::Postgresql => "POSTGRES",
		DatabaseEngine::Mysql => "MYSQL",
		DatabaseEngine::Sqlite => "SQLITE",
	};
	match version {
		Some(v) => format!("{prefix}_{v}"),
		None => match engine {
			DatabaseEngine::Postgresql => "POSTGRES_16".to_string(),
			DatabaseEngine::Mysql => "MYSQL_8_0".to_string(),
			DatabaseEngine::Sqlite => "SQLITE_3".to_string(),
		},
	}
}

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

	let namespace = match app.namespace() {
		Some(ns) => ns,
		None => return vec![], // namespace required for database resources
	};
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
	let db_password = generate_random_password(24);
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
	let secret = build_db_credentials_secret(app_name, namespace, &db_user, &db_password);

	vec![
		DatabaseResource::StatefulSet(Box::new(stateful_set)),
		DatabaseResource::Pvc(Box::new(pvc)),
		DatabaseResource::ConfigMap(config_map),
		DatabaseResource::Secret(secret),
	]
}

/// Build AWS RDS resources via ACK (DBInstance DynamicObject + credentials Secret).
fn build_aws_rds(app_name: &str, namespace: &str, db: &DatabaseSpec) -> Vec<DatabaseResource> {
	let engine = engine_to_string(&db.engine);
	let engine_version = db.version.as_deref().unwrap_or("16");
	let instance_class = db
		.instance_class
		.as_deref()
		.unwrap_or("db.t3.micro")
		.to_string();
	let allocated_storage = db.storage_gb.unwrap_or(20);

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
		data: serde_json::json!({
			"spec": {
				"engine": engine,
				"engineVersion": engine_version,
				"dbInstanceClass": instance_class,
				"allocatedStorage": allocated_storage,
				"masterUsername": app_name,
				"masterUserPassword": {
					"namespace": namespace,
					"name": format!("{app_name}-db-credentials"),
					"key": "password"
				},
				"dbName": format!("{app_name}_db")
			}
		}),
	};

	let password = generate_random_password(24);
	let secret = build_db_credentials_secret(app_name, namespace, app_name, &password);

	vec![
		DatabaseResource::Dynamic(db_instance),
		DatabaseResource::Secret(secret),
	]
}

/// Build GCP Cloud SQL resources via Config Connector
/// (SQLInstance, SQLDatabase, SQLUser DynamicObjects + credentials Secret).
fn build_gcp_cloud_sql(
	app_name: &str,
	namespace: &str,
	db: &DatabaseSpec,
	storage_gb: i32,
) -> Vec<DatabaseResource> {
	let database_version = engine_to_gcp_version(&db.engine, db.version.as_deref());
	let tier = db
		.instance_class
		.as_deref()
		.unwrap_or("db-f1-micro")
		.to_string();

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
		data: serde_json::json!({
			"spec": {
				"databaseVersion": database_version,
				"region": "us-central1",
				"settings": {
					"tier": tier,
					"dataDiskSizeGb": storage_gb,
					"ipConfiguration": {
						"ipv4Enabled": true
					}
				}
			}
		}),
	};

	let instance_ref_name = format!("{app_name}-sql-instance");

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
		data: serde_json::json!({
			"spec": {
				"instanceRef": {
					"name": instance_ref_name
				},
				"charset": "UTF8",
				"collation": "en_US.UTF8"
			}
		}),
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
		data: serde_json::json!({
			"spec": {
				"instanceRef": {
					"name": instance_ref_name
				},
				"password": {
					"valueFrom": {
						"secretKeyRef": {
							"name": format!("{app_name}-db-credentials"),
							"key": "password"
						}
					}
				}
			}
		}),
	};

	let password = generate_random_password(24);
	let secret = build_db_credentials_secret(app_name, namespace, app_name, &password);

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
			"reinhardt-cloud-operator".to_string(),
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
	use reinhardt_cloud_types::crd::{DatabaseEngine, ReinhardtAppSpec};
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
				database: Some(db_spec),
				..Default::default()
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
				..Default::default()
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
	fn aws_dynamic_object_has_rds_type_and_non_empty_spec() {
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
			assert!(obj.data["spec"].is_object(), "spec must be non-empty");
			assert!(obj.data["spec"]["engine"].is_string());
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
	fn gcp_dynamic_objects_have_config_connector_types_and_non_empty_specs() {
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
			assert!(
				obj.data["spec"].is_object(),
				"SQLInstance spec must be non-empty"
			);
		} else {
			panic!("Expected Dynamic as first resource");
		}

		if let DatabaseResource::Dynamic(obj) = &resources[1] {
			let types = obj.types.as_ref().unwrap();
			assert_eq!(types.kind, "SQLDatabase");
			assert!(
				obj.data["spec"].is_object(),
				"SQLDatabase spec must be non-empty"
			);
		} else {
			panic!("Expected Dynamic as second resource");
		}

		if let DatabaseResource::Dynamic(obj) = &resources[2] {
			let types = obj.types.as_ref().unwrap();
			assert_eq!(types.kind, "SQLUser");
			assert!(
				obj.data["spec"].is_object(),
				"SQLUser spec must be non-empty"
			);
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

	// --- Helper function tests ---

	#[rstest]
	#[case(DatabaseEngine::Postgresql, "postgres")]
	#[case(DatabaseEngine::Mysql, "mysql")]
	#[case(DatabaseEngine::Sqlite, "sqlite")]
	fn engine_to_string_maps_correctly(#[case] engine: DatabaseEngine, #[case] expected: &str) {
		// Act
		let result = engine_to_string(&engine);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(DatabaseEngine::Postgresql, Some("15"), "POSTGRES_15")]
	#[case(DatabaseEngine::Postgresql, None, "POSTGRES_16")]
	#[case(DatabaseEngine::Mysql, Some("8_0"), "MYSQL_8_0")]
	#[case(DatabaseEngine::Mysql, None, "MYSQL_8_0")]
	fn engine_to_gcp_version_maps_correctly(
		#[case] engine: DatabaseEngine,
		#[case] version: Option<&str>,
		#[case] expected: &str,
	) {
		// Act
		let result = engine_to_gcp_version(&engine, version);

		// Assert
		assert_eq!(result, expected);
	}

	// --- AWS RDS field mapping tests ---

	#[rstest]
	fn aws_rds_maps_postgresql_engine() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: None,
			version: Some("15".to_string()),
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::aws_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let spec = &obj.data["spec"];
			assert_eq!(spec["engine"], "postgres");
			assert_eq!(spec["engineVersion"], "15");
			assert_eq!(spec["dbName"], "myapp_db");
			assert_eq!(spec["masterUsername"], "myapp");
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn aws_rds_maps_mysql_engine() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Mysql,
			instance_class: None,
			storage_gb: None,
			version: Some("8.0".to_string()),
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::aws_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let spec = &obj.data["spec"];
			assert_eq!(spec["engine"], "mysql");
			assert_eq!(spec["engineVersion"], "8.0");
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn aws_rds_uses_custom_instance_class() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: Some("db.r5.large".to_string()),
			storage_gb: Some(100),
			version: Some("16".to_string()),
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::aws_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let spec = &obj.data["spec"];
			assert_eq!(spec["dbInstanceClass"], "db.r5.large");
			assert_eq!(spec["allocatedStorage"], 100);
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn aws_rds_uses_defaults_when_optional_fields_unset() {
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
			let spec = &obj.data["spec"];
			assert_eq!(spec["dbInstanceClass"], "db.t3.micro");
			assert_eq!(spec["allocatedStorage"], 20);
			assert_eq!(spec["engineVersion"], "16");
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn aws_rds_master_user_password_references_credentials_secret() {
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
			let pwd_ref = &obj.data["spec"]["masterUserPassword"];
			assert_eq!(pwd_ref["namespace"], "default");
			assert_eq!(pwd_ref["name"], "myapp-db-credentials");
			assert_eq!(pwd_ref["key"], "password");
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	// --- GCP Cloud SQL field mapping tests ---

	#[rstest]
	fn gcp_sql_instance_maps_postgresql_version() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(30),
			version: Some("15".to_string()),
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::gcp_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let spec = &obj.data["spec"];
			assert_eq!(spec["databaseVersion"], "POSTGRES_15");
			assert_eq!(spec["region"], "us-central1");
			assert_eq!(spec["settings"]["dataDiskSizeGb"], 30);
			assert_eq!(spec["settings"]["tier"], "db-f1-micro");
			assert_eq!(spec["settings"]["ipConfiguration"]["ipv4Enabled"], true);
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn gcp_sql_instance_maps_mysql_version() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Mysql,
			instance_class: None,
			storage_gb: None,
			version: Some("8_0".to_string()),
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::gcp_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			assert_eq!(obj.data["spec"]["databaseVersion"], "MYSQL_8_0");
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn gcp_sql_instance_uses_custom_tier() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: Some("db-custom-2-8192".to_string()),
			storage_gb: Some(50),
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::gcp_defaults();

		// Act
		let resources = infer_database_resources(&app, &platform);

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let settings = &obj.data["spec"]["settings"];
			assert_eq!(settings["tier"], "db-custom-2-8192");
			assert_eq!(settings["dataDiskSizeGb"], 50);
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn gcp_sql_instance_defaults_version_when_unset() {
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
			assert_eq!(obj.data["spec"]["databaseVersion"], "POSTGRES_16");
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}

	#[rstest]
	fn gcp_sql_database_references_instance() {
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
		if let DatabaseResource::Dynamic(obj) = &resources[1] {
			let spec = &obj.data["spec"];
			assert_eq!(spec["instanceRef"]["name"], "myapp-sql-instance");
			assert_eq!(spec["charset"], "UTF8");
			assert_eq!(spec["collation"], "en_US.UTF8");
		} else {
			panic!("Expected Dynamic as second resource");
		}
	}

	#[rstest]
	fn gcp_sql_user_references_instance_and_credentials() {
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
		if let DatabaseResource::Dynamic(obj) = &resources[2] {
			let spec = &obj.data["spec"];
			assert_eq!(spec["instanceRef"]["name"], "myapp-sql-instance");
			let secret_ref = &spec["password"]["valueFrom"]["secretKeyRef"];
			assert_eq!(secret_ref["name"], "myapp-db-credentials");
			assert_eq!(secret_ref["key"], "password");
		} else {
			panic!("Expected Dynamic as third resource");
		}
	}
}
