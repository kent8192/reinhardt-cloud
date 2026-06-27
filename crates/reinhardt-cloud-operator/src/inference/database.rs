//! Database resource inference for multi-platform provisioning.
//!
//! Generates the appropriate Kubernetes resources for database provisioning
//! based on the target platform: on-premise uses StatefulSets with PVCs,
//! AWS uses ACK `DynamicObject`, and GCP uses Config Connector `DynamicObject`.
//!
//! This module is consumed by the reconciler's explicit-database branch:
//! when `spec.database` is set, the reconciler calls
//! `infer_database_resources` and applies the returned resources via
//! server-side apply.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{
	ConfigMap, Container, ContainerPort, EnvFromSource, PersistentVolumeClaim,
	PersistentVolumeClaimSpec, PodSpec, PodTemplateSpec, Secret, SecretEnvSource, Service,
	ServicePort, ServiceSpec, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::ResourceExt;
use kube::api::{DynamicObject, TypeMeta};
use reinhardt_cloud_types::crd::{DatabaseEngine, DatabaseSpec, Project};

use super::platform::{Platform, PlatformConfig};
use super::secrets::{build_db_credentials_secret, generate_random_password};
use crate::error::Error;

const AWS_DATABASE_INSTANCE_CLASSES: &[&str] = &[
	"db.t3.micro",
	"db.t3.small",
	"db.t3.medium",
	"db.t3.large",
	"db.r5.large",
];

const GCP_DATABASE_INSTANCE_CLASSES: &[&str] = &[
	"db-f1-micro",
	"db-g1-small",
	"db-custom-1-3840",
	"db-custom-2-8192",
];

/// Map a `DatabaseEngine` variant to the lowercase engine name used by AWS RDS.
///
/// Returns `None` for SQLite since it is not supported by cloud providers.
fn engine_to_string(engine: &DatabaseEngine) -> Option<&str> {
	match engine {
		DatabaseEngine::Postgresql => Some("postgres"),
		DatabaseEngine::Mysql => Some("mysql"),
		DatabaseEngine::Sqlite => None,
	}
}

/// Return the default engine version for AWS RDS based on the engine type.
fn default_engine_version(engine: &DatabaseEngine) -> &str {
	match engine {
		DatabaseEngine::Postgresql => "16",
		DatabaseEngine::Mysql => "8.0",
		DatabaseEngine::Sqlite => "",
	}
}

/// Return the maximum identifier length for a given database engine.
///
/// MySQL usernames are limited to 32 characters, PostgreSQL identifiers
/// to 63 characters, and SQLite has no practical limit (use 63 as
/// a reasonable upper bound).
fn max_identifier_len(engine: &DatabaseEngine) -> usize {
	match engine {
		DatabaseEngine::Mysql => 32,
		DatabaseEngine::Postgresql | DatabaseEngine::Sqlite => 63,
	}
}

/// Sanitize a project name for use as a database username or db name.
///
/// The identifier must start with a letter, contain only ASCII alphanumeric
/// characters and underscores, and be truncated to the engine-specific
/// maximum length (MySQL: 32, PostgreSQL: 63).
fn sanitize_identifier(name: &str, engine: &DatabaseEngine) -> String {
	let sanitized: String = name
		.chars()
		.map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
		.collect();
	// Ensure it starts with a letter
	let sanitized = if sanitized.starts_with(|c: char| c.is_ascii_alphabetic()) {
		sanitized
	} else {
		format!("app_{sanitized}")
	};
	let max_len = max_identifier_len(engine);
	sanitized.chars().take(max_len).collect()
}

/// Build a GCP Cloud SQL `databaseVersion` string from engine and optional
/// version (e.g. `POSTGRES_16`, `MYSQL_8_0`).
///
/// Returns `None` for SQLite since it is not supported by Cloud SQL.
/// Version strings are normalized: dots are replaced with underscores
/// to produce valid GCP version identifiers (e.g. `8.0` becomes `MYSQL_8_0`).
fn engine_to_gcp_version(engine: &DatabaseEngine, version: Option<&str>) -> Option<String> {
	let prefix = match engine {
		DatabaseEngine::Postgresql => "POSTGRES",
		DatabaseEngine::Mysql => "MYSQL",
		DatabaseEngine::Sqlite => return None,
	};
	match version {
		Some(v) => {
			// Normalize dots to underscores for GCP format
			let normalized = v.replace('.', "_");
			Some(format!("{prefix}_{normalized}"))
		}
		None => match engine {
			DatabaseEngine::Postgresql => Some("POSTGRES_16".to_string()),
			DatabaseEngine::Mysql => Some("MYSQL_8_0".to_string()),
			DatabaseEngine::Sqlite => None,
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
	/// A Service exposing the database within the cluster (on-premise).
	Service(Box<Service>),
	/// A ConfigMap for database initialisation scripts.
	ConfigMap(Box<ConfigMap>),
	/// A Secret containing database credentials.
	Secret(Box<Secret>),
	/// A dynamic object for cloud provider CRDs (ACK, Config Connector).
	Dynamic(Box<DynamicObject>),
}

/// Infer database resources based on the app spec and the target platform.
///
/// Returns an empty `Vec` when the app has no database configuration.
pub(crate) fn infer_database_resources(
	app: &Project,
	platform: &PlatformConfig,
) -> Result<Vec<DatabaseResource>, Error> {
	let db = match &app.spec.database {
		Some(db) => db,
		None => return Ok(vec![]),
	};
	validate_database_for_platform(db, platform)?;

	let namespace = match app.namespace() {
		Some(ns) => ns,
		None => return Ok(vec![]), // namespace required for database resources
	};
	let project_name = app.name_any();
	let storage_gb = db
		.storage_gb
		.unwrap_or(platform.defaults.database.storage_gb);

	let resources = match platform.platform {
		Platform::Onpremise => build_onprem_postgres(&project_name, &namespace, db, storage_gb),
		Platform::Aws => build_aws_rds(&project_name, &namespace, db, platform),
		Platform::Gcp => build_gcp_cloud_sql(&project_name, &namespace, db, storage_gb, platform),
	};
	Ok(resources)
}

fn validate_database_for_platform(
	db: &DatabaseSpec,
	platform: &PlatformConfig,
) -> Result<(), Error> {
	db.validate().map_err(Error::DatabaseProvisioning)?;
	let Some(instance_class) = db.instance_class.as_deref() else {
		return Ok(());
	};
	let allowed = match platform.platform {
		Platform::Aws => AWS_DATABASE_INSTANCE_CLASSES,
		Platform::Gcp => GCP_DATABASE_INSTANCE_CLASSES,
		Platform::Onpremise => {
			return Err(Error::DatabaseProvisioning(
				"database.instance_class is not supported for onpremise databases".to_string(),
			));
		}
	};
	if allowed.contains(&instance_class) {
		Ok(())
	} else {
		Err(Error::DatabaseProvisioning(format!(
			"database.instance_class '{instance_class}' is not allowed for {:?}; allowed values: {}",
			platform.platform,
			allowed.join(", ")
		)))
	}
}

/// Build on-premise PostgreSQL resources: StatefulSet + PVC + ConfigMap + Secret.
fn build_onprem_postgres(
	project_name: &str,
	namespace: &str,
	db: &DatabaseSpec,
	storage_gb: i32,
) -> Vec<DatabaseResource> {
	// Both identifiers use replace('-', "_") to produce valid SQL identifiers.
	// env_vars.rs derives REINHARDT_DATABASE_NAME and REINHARDT_DATABASE_USER
	// using the same convention, ensuring the injected connection env vars
	// match the credentials created here.
	let sanitized_name = project_name.replace('-', "_");
	let db_name = format!("{sanitized_name}_db");
	let db_user = sanitized_name.clone();
	let db_password = generate_random_password(24);
	let pg_version = db.version.as_deref().unwrap_or("16");
	let labels = standard_db_labels(project_name);
	let stateful_set_selector_labels = legacy_stateful_set_selector_labels(project_name);
	let service_selector_labels = db_selector_labels(project_name);

	// StatefulSet running postgres
	let stateful_set = StatefulSet {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-db")),
			namespace: Some(namespace.to_string()),
			labels: Some(labels.clone()),
			..Default::default()
		},
		spec: Some(StatefulSetSpec {
			replicas: Some(1),
			service_name: Some(format!("{project_name}-db")),
			selector: LabelSelector {
				match_labels: Some(stateful_set_selector_labels),
				..Default::default()
			},
			template: PodTemplateSpec {
				metadata: Some(ObjectMeta {
					labels: Some(labels.clone()),
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
						env_from: Some(vec![EnvFromSource {
							secret_ref: Some(SecretEnvSource {
								name: format!("{project_name}-db-credentials"),
								..Default::default()
							}),
							..Default::default()
						}]),
						..Default::default()
					}],
					volumes: Some(vec![Volume {
						name: "data".to_string(),
						persistent_volume_claim: Some(
							k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
								claim_name: format!("{project_name}-db-data"),
								..Default::default()
							},
						),
						..Default::default()
					}]),
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
			name: Some(format!("{project_name}-db-data")),
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

	// Headless Service exposing the database within the cluster.
	// The Service name matches the host used by build_database_env_vars_from_secret
	// so that the injected REINHARDT_DATABASE_HOST resolves correctly.
	// cluster_ip = "None" makes this a headless Service so that DNS resolves
	// directly to the pod IP (required for StatefulSet clients).
	let service = Service {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-db")),
			namespace: Some(namespace.to_string()),
			labels: Some(labels.clone()),
			..Default::default()
		},
		spec: Some(ServiceSpec {
			type_: Some("ClusterIP".to_string()),
			cluster_ip: Some("None".to_string()),
			selector: Some(service_selector_labels),
			ports: Some(vec![ServicePort {
				port: 5432,
				target_port: Some(IntOrString::Int(5432)),
				name: Some("postgres".to_string()),
				..Default::default()
			}]),
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
			name: Some(format!("{project_name}-db-init")),
			namespace: Some(namespace.to_string()),
			labels: Some(labels),
			..Default::default()
		},
		data: Some(BTreeMap::from([("init.sql".to_string(), init_sql)])),
		..Default::default()
	};

	// Credentials secret
	let secret =
		build_db_credentials_secret(project_name, namespace, &db_user, &db_password, &db_name);

	vec![
		DatabaseResource::StatefulSet(Box::new(stateful_set)),
		DatabaseResource::Pvc(Box::new(pvc)),
		DatabaseResource::Service(Box::new(service)),
		DatabaseResource::ConfigMap(Box::new(config_map)),
		DatabaseResource::Secret(Box::new(secret)),
	]
}

/// Build AWS RDS resources via ACK (DBInstance DynamicObject + credentials Secret).
///
/// Returns an empty `Vec` for SQLite since AWS RDS does not support it.
fn build_aws_rds(
	project_name: &str,
	namespace: &str,
	db: &DatabaseSpec,
	platform: &PlatformConfig,
) -> Vec<DatabaseResource> {
	let engine = match engine_to_string(&db.engine) {
		Some(e) => e,
		// SQLite is not supported by AWS RDS — skip resource generation
		None => return vec![],
	};
	let engine_version = db
		.version
		.as_deref()
		.unwrap_or_else(|| default_engine_version(&db.engine));
	let instance_class = db
		.instance_class
		.as_deref()
		.unwrap_or(&platform.defaults.database.instance_class)
		.to_string();
	let allocated_storage = db
		.storage_gb
		.unwrap_or(platform.defaults.database.storage_gb);
	let master_username = sanitize_identifier(project_name, &db.engine);
	let db_name = sanitize_identifier(&format!("{project_name}_db"), &db.engine);

	let db_instance = DynamicObject {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-rds")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_db_labels(project_name)),
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
				"masterUsername": master_username,
				"masterUserPassword": {
					"namespace": namespace,
					"name": format!("{project_name}-db-credentials"),
					"key": "password"
				},
				"dbName": db_name
			}
		}),
	};

	let password = generate_random_password(24);
	let secret = build_db_credentials_secret(
		project_name,
		namespace,
		&master_username,
		&password,
		&db_name,
	);

	vec![
		DatabaseResource::Dynamic(Box::new(db_instance)),
		DatabaseResource::Secret(Box::new(secret)),
	]
}

/// Build GCP Cloud SQL resources via Config Connector
/// (SQLInstance, SQLDatabase, SQLUser DynamicObjects + credentials Secret).
///
/// Returns an empty `Vec` for SQLite since Cloud SQL does not support it.
fn build_gcp_cloud_sql(
	project_name: &str,
	namespace: &str,
	db: &DatabaseSpec,
	storage_gb: i32,
	platform: &PlatformConfig,
) -> Vec<DatabaseResource> {
	let database_version = match engine_to_gcp_version(&db.engine, db.version.as_deref()) {
		Some(v) => v,
		// SQLite is not supported by GCP Cloud SQL — skip resource generation
		None => return vec![],
	};
	let tier = db
		.instance_class
		.as_deref()
		.unwrap_or("db-f1-micro")
		.to_string();
	let region = &platform.defaults.database.region;
	let db_name = sanitize_identifier(&format!("{project_name}_db"), &db.engine);

	let sql_instance = DynamicObject {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-sql-instance")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_db_labels(project_name)),
			..Default::default()
		},
		types: Some(TypeMeta {
			api_version: "sql.cnrm.cloud.google.com/v1beta1".to_string(),
			kind: "SQLInstance".to_string(),
		}),
		data: serde_json::json!({
			"spec": {
				"databaseVersion": database_version,
				"region": region,
				"settings": {
					"tier": tier,
					"dataDiskSizeGb": storage_gb,
					"ipConfiguration": {
						"ipv4Enabled": false
					}
				}
			}
		}),
	};

	let instance_ref_name = format!("{project_name}-sql-instance");

	let sql_database = DynamicObject {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-sql-database")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_db_labels(project_name)),
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
			name: Some(format!("{project_name}-sql-user")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_db_labels(project_name)),
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
							"name": format!("{project_name}-db-credentials"),
							"key": "password"
						}
					}
				}
			}
		}),
	};

	let sanitized_user = sanitize_identifier(project_name, &db.engine);
	let password = generate_random_password(24);
	let secret = build_db_credentials_secret(
		project_name,
		namespace,
		&sanitized_user,
		&password,
		&db_name,
	);

	vec![
		DatabaseResource::Dynamic(Box::new(sql_instance)),
		DatabaseResource::Dynamic(Box::new(sql_database)),
		DatabaseResource::Dynamic(Box::new(sql_user)),
		DatabaseResource::Secret(Box::new(secret)),
	]
}

fn standard_db_labels(project_name: &str) -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			"app.kubernetes.io/name".to_string(),
			project_name.to_string(),
		),
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

fn db_selector_labels(project_name: &str) -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			"app.kubernetes.io/name".to_string(),
			project_name.to_string(),
		),
		(
			"app.kubernetes.io/component".to_string(),
			"database".to_string(),
		),
	])
}

fn legacy_stateful_set_selector_labels(project_name: &str) -> BTreeMap<String, String> {
	BTreeMap::from([(
		"app.kubernetes.io/name".to_string(),
		project_name.to_string(),
	)])
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::crd::{DatabaseEngine, ProjectSpec};
	use rstest::rstest;

	fn make_app_with_db(name: &str, db_spec: DatabaseSpec) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
				image: "myapp:latest".to_string(),
				database: Some(db_spec),
				..Default::default()
			},
			status: None,
		}
	}

	fn make_app_without_db(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		assert!(resources.is_empty());
	}

	#[rstest]
	fn invalid_database_spec_returns_error() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: Some("db.r7i.48xlarge".to_string()),
			storage_gb: Some(2_000_000_000),
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::aws_defaults();

		// Act
		let result = infer_database_resources(&app, &platform);

		// Assert
		assert!(matches!(result, Err(Error::DatabaseProvisioning(_))));
	}

	#[rstest]
	fn database_instance_class_must_match_target_platform() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: Some("db-custom-2-8192".to_string()),
			storage_gb: Some(20),
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::aws_defaults();

		// Act
		let result = infer_database_resources(&app, &platform);

		// Assert
		assert!(matches!(result, Err(Error::DatabaseProvisioning(_))));
	}

	#[rstest]
	fn onprem_generates_five_resources() {
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		assert_eq!(resources.len(), 5);
		assert!(matches!(&resources[0], DatabaseResource::StatefulSet(..)));
		assert!(matches!(&resources[1], DatabaseResource::Pvc(..)));
		assert!(matches!(&resources[2], DatabaseResource::Service(_)));
		assert!(matches!(&resources[3], DatabaseResource::ConfigMap(_)));
		assert!(matches!(&resources[4], DatabaseResource::Secret(_)));
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
	fn onprem_statefulset_mounts_generated_pvc() {
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		let DatabaseResource::StatefulSet(ss) = &resources[0] else {
			panic!("Expected StatefulSet as first resource");
		};
		let pod_spec = ss.spec.as_ref().unwrap().template.spec.as_ref().unwrap();
		let volume = pod_spec
			.volumes
			.as_ref()
			.unwrap()
			.iter()
			.find(|volume| volume.name == "data")
			.unwrap();
		assert_eq!(
			volume.persistent_volume_claim.as_ref().unwrap().claim_name,
			"myapp-db-data"
		);
		assert_eq!(
			pod_spec.containers[0].volume_mounts.as_ref().unwrap()[0].name,
			"data"
		);
	}

	#[rstest]
	fn onprem_statefulset_injects_credentials_secret() {
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		let DatabaseResource::StatefulSet(ss) = &resources[0] else {
			panic!("Expected StatefulSet as first resource");
		};
		let spec = ss.spec.as_ref().unwrap();
		assert_eq!(spec.service_name.as_deref(), Some("myapp-db"));
		let container = &spec.template.spec.as_ref().unwrap().containers[0];
		let secret_ref = container.env_from.as_ref().unwrap()[0]
			.secret_ref
			.as_ref()
			.unwrap();
		assert_eq!(secret_ref.name, "myapp-db-credentials");
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
	fn onprem_service_exposes_postgres_port() {
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		if let DatabaseResource::Service(svc) = &resources[2] {
			assert_eq!(svc.metadata.name.as_deref(), Some("myapp-db"));
			let spec = svc.spec.as_ref().unwrap();
			let port = &spec.ports.as_ref().unwrap()[0];
			assert_eq!(port.port, 5432);
			let selector = spec.selector.as_ref().unwrap();
			assert_eq!(
				selector.get("app.kubernetes.io/name").map(String::as_str),
				Some("myapp")
			);
			assert_eq!(
				selector
					.get("app.kubernetes.io/component")
					.map(String::as_str),
				Some("database")
			);
		} else {
			panic!("Expected Service as third resource");
		}
	}

	#[rstest]
	fn onprem_statefulset_keeps_legacy_selector_and_service_selects_database_component() {
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		let DatabaseResource::StatefulSet(ss) = &resources[0] else {
			panic!("Expected StatefulSet as first resource");
		};
		let stateful_set_spec = ss.spec.as_ref().unwrap();
		let selector = stateful_set_spec.selector.match_labels.as_ref().unwrap();
		assert_eq!(
			selector.get("app.kubernetes.io/name").map(String::as_str),
			Some("myapp")
		);
		assert_eq!(selector.len(), 1);

		let pod_labels = stateful_set_spec
			.template
			.metadata
			.as_ref()
			.unwrap()
			.labels
			.as_ref()
			.unwrap();
		assert_eq!(
			pod_labels
				.get("app.kubernetes.io/component")
				.map(String::as_str),
			Some("database")
		);

		let DatabaseResource::Service(svc) = &resources[2] else {
			panic!("Expected Service as third resource");
		};
		assert_eq!(
			svc.spec
				.as_ref()
				.unwrap()
				.selector
				.as_ref()
				.unwrap()
				.get("app.kubernetes.io/component")
				.map(String::as_str),
			Some("database")
		);
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
	#[case(DatabaseEngine::Postgresql, Some("postgres"))]
	#[case(DatabaseEngine::Mysql, Some("mysql"))]
	#[case(DatabaseEngine::Sqlite, None)]
	fn engine_to_string_maps_correctly(
		#[case] engine: DatabaseEngine,
		#[case] expected: Option<&str>,
	) {
		// Act
		let result = engine_to_string(&engine);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(DatabaseEngine::Postgresql, Some("15"), Some("POSTGRES_15"))]
	#[case(DatabaseEngine::Postgresql, None, Some("POSTGRES_16"))]
	#[case(DatabaseEngine::Mysql, Some("8.0"), Some("MYSQL_8_0"))]
	#[case(DatabaseEngine::Mysql, None, Some("MYSQL_8_0"))]
	#[case(DatabaseEngine::Sqlite, None, None)]
	fn engine_to_gcp_version_maps_correctly(
		#[case] engine: DatabaseEngine,
		#[case] version: Option<&str>,
		#[case] expected: Option<&str>,
	) {
		// Act
		let result = engine_to_gcp_version(&engine, version);

		// Assert
		assert_eq!(result.as_deref(), expected);
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
	fn aws_rds_uses_platform_defaults_when_optional_fields_unset() {
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert — values should come from platform.defaults.database
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let spec = &obj.data["spec"];
			assert_eq!(
				spec["dbInstanceClass"],
				platform.defaults.database.instance_class
			);
			assert_eq!(
				spec["allocatedStorage"],
				platform.defaults.database.storage_gb
			);
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			let spec = &obj.data["spec"];
			assert_eq!(spec["databaseVersion"], "POSTGRES_15");
			assert_eq!(spec["region"], "us-central1");
			assert_eq!(spec["settings"]["dataDiskSizeGb"], 30);
			assert_eq!(spec["settings"]["tier"], "db-f1-micro");
			assert_eq!(spec["settings"]["ipConfiguration"]["ipv4Enabled"], false);
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

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

	// --- SQLite cloud provider tests ---

	#[rstest]
	fn aws_sqlite_returns_empty_resources() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Sqlite,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::aws_defaults();

		// Act
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		assert!(resources.is_empty());
	}

	#[rstest]
	fn gcp_sqlite_returns_empty_resources() {
		// Arrange
		let db_spec = DatabaseSpec {
			engine: DatabaseEngine::Sqlite,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		let app = make_app_with_db("myapp", db_spec);
		let platform = PlatformConfig::gcp_defaults();

		// Act
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		assert!(resources.is_empty());
	}

	// --- Sanitization tests ---

	#[rstest]
	#[case("myapp", "myapp")]
	#[case("my-app", "my_app")]
	#[case("my.app.name", "my_app_name")]
	#[case("123app", "app_123app")]
	fn sanitize_identifier_normalizes_names(#[case] input: &str, #[case] expected: &str) {
		// Act
		let result = sanitize_identifier(input, &DatabaseEngine::Postgresql);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	fn sanitize_identifier_truncates_to_mysql_limit() {
		// Arrange
		let long_name = "a".repeat(64);

		// Act
		let result = sanitize_identifier(&long_name, &DatabaseEngine::Mysql);

		// Assert — MySQL limit is 32 characters
		assert_eq!(result.len(), 32);
	}

	#[rstest]
	fn sanitize_identifier_truncates_to_postgresql_limit() {
		// Arrange
		let long_name = "a".repeat(128);

		// Act
		let result = sanitize_identifier(&long_name, &DatabaseEngine::Postgresql);

		// Assert — PostgreSQL limit is 63 characters
		assert_eq!(result.len(), 63);
	}

	// --- Region configuration tests ---

	#[rstest]
	fn gcp_uses_platform_region() {
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
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		if let DatabaseResource::Dynamic(obj) = &resources[0] {
			assert_eq!(obj.data["spec"]["region"], "us-central1");
		} else {
			panic!("Expected Dynamic as first resource");
		}
	}
}
