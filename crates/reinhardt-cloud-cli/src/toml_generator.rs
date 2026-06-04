//! Generates `reinhardt-cloud.toml` content from project metadata.

use crate::feature_detector::{InfraSignals, ProjectMetadata};
use crate::settings_reader::DatabaseConfig;
use reinhardt_cloud_core::infrastructure_derivation::{
	InfrastructureDerivationInput, derive_infrastructure_spec,
};
use reinhardt_cloud_types::crd::infrastructure::InfrastructureSpec;
use reinhardt_cloud_types::introspect;
use reinhardt_cloud_types::reinhardt_cloud_toml::{
	AppSection, AuthSection, CacheSection, DatabaseSection, HealthSection, ReinhardtCloudToml,
	ScaleSection, ServicesSection, StorageSection, WorkerSection,
};

const DEFAULT_HEALTH_PATH: &str = "/api/healthz/";
const DEFAULT_APP_PORT: i32 = 8000;
const DEFAULT_HEALTH_INTERVAL_SECONDS: i32 = 10;
const DEFAULT_SERVICE_PORT: i32 = 80;
const DEFAULT_MIN_REPLICAS: i32 = 2;
const DEFAULT_MAX_REPLICAS: i32 = 6;
const DEFAULT_SCALE_METRIC: &str = "cpu";
const DEFAULT_SCALE_TARGET_VALUE: i32 = 70;

/// Generate a `ReinhardtCloudToml` from project metadata and optional database config
pub(crate) fn generate_config(
	metadata: &ProjectMetadata,
	db_config: Option<&DatabaseConfig>,
) -> Result<ReinhardtCloudToml, String> {
	let (has_database, db_engine) = resolve_database(metadata, db_config);
	let infrastructure = derive_infrastructure_spec(InfrastructureDerivationInput {
		app_name: metadata.name.clone(),
		signals: convert_infra_signals(&metadata.signals, has_database.then(|| db_engine.clone())),
		explicit: None,
		typed_secret_refs: Vec::new(),
	})
	.map_err(|e| e.to_string())?;

	Ok(build_config(
		metadata,
		db_config,
		has_database,
		db_engine,
		infrastructure,
	))
}

pub(crate) fn generate_config_preserving_explicit_infrastructure(
	metadata: &ProjectMetadata,
	db_config: Option<&DatabaseConfig>,
	explicit_infrastructure: Option<&InfrastructureSpec>,
) -> Result<ReinhardtCloudToml, String> {
	if let Some(infrastructure) = explicit_infrastructure {
		validate_infrastructure(infrastructure)?;
	}

	match generate_config(metadata, db_config) {
		Ok(config) => Ok(config),
		Err(error) => {
			let Some(infrastructure) = explicit_infrastructure else {
				return Err(error);
			};
			let (has_database, db_engine) = resolve_database(metadata, db_config);
			Ok(build_config(
				metadata,
				db_config,
				has_database,
				db_engine,
				Some(infrastructure.clone()),
			))
		}
	}
}

fn resolve_database(
	metadata: &ProjectMetadata,
	db_config: Option<&DatabaseConfig>,
) -> (bool, String) {
	let has_database = metadata.signals.database.is_some() || db_config.is_some();
	let db_engine = db_config
		.map(|d| d.engine.clone())
		.or_else(|| metadata.signals.database.clone())
		.unwrap_or_else(|| "postgresql".to_owned());

	(has_database, db_engine)
}

fn build_config(
	metadata: &ProjectMetadata,
	db_config: Option<&DatabaseConfig>,
	has_database: bool,
	db_engine: String,
	infrastructure: Option<InfrastructureSpec>,
) -> ReinhardtCloudToml {
	let mut config = ReinhardtCloudToml {
		app: AppSection {
			name: metadata.name.clone(),
			image: format!("{}:latest", metadata.name),
		},
		database: if has_database {
			Some(DatabaseSection {
				engine: db_engine,
				..Default::default()
			})
		} else {
			None
		},
		auth: if metadata.signals.jwt {
			Some(AuthSection { jwt: true })
		} else {
			None
		},
		health: Some(HealthSection {
			path: Some(DEFAULT_HEALTH_PATH.to_owned()),
			port: Some(DEFAULT_APP_PORT),
			interval_seconds: Some(DEFAULT_HEALTH_INTERVAL_SECONDS),
		}),
		services: Some(ServicesSection {
			port: Some(DEFAULT_SERVICE_PORT),
			target_port: Some(DEFAULT_APP_PORT),
			ingress_host: None,
		}),
		scale: Some(ScaleSection {
			min_replicas: Some(DEFAULT_MIN_REPLICAS),
			max_replicas: Some(DEFAULT_MAX_REPLICAS),
			metric: Some(DEFAULT_SCALE_METRIC.to_owned()),
			target_value: Some(DEFAULT_SCALE_TARGET_VALUE),
		}),
		cache: metadata.signals.cache.as_ref().map(|backend| CacheSection {
			backend: backend.clone(),
			..Default::default()
		}),
		worker: if metadata.signals.background_worker {
			Some(WorkerSection::default())
		} else {
			None
		},
		storage: if metadata.signals.object_storage {
			Some(StorageSection::default())
		} else {
			None
		},
		infrastructure,
		..Default::default()
	};

	if let Some(db_config) = db_config {
		config.env.extend(db_config.deployment_env());
	}

	config
}

fn validate_infrastructure(infrastructure: &InfrastructureSpec) -> Result<(), String> {
	infrastructure.validate().map_err(|errors| {
		errors
			.into_iter()
			.map(|error| error.to_string())
			.collect::<Vec<_>>()
			.join("; ")
	})
}

fn convert_infra_signals(
	signals: &InfraSignals,
	effective_database: Option<String>,
) -> introspect::InfraSignals {
	introspect::InfraSignals {
		database: effective_database,
		cache: signals.cache.clone(),
		websocket: signals.websocket,
		background_worker: signals.background_worker,
		grpc: signals.grpc,
		storage: None,
		mail: None,
		session_backend: signals.sessions.then(|| "db".to_string()),
		graphql: signals.graphql,
		admin_panel: false,
		i18n: false,
		pages: signals.pages,
	}
}

/// Serialize `ReinhardtCloudToml` to a TOML string with a header comment
pub(crate) fn generate_reinhardt_cloud_toml_string(config: &ReinhardtCloudToml) -> String {
	let output = toml::to_string_pretty(config).unwrap_or_default();

	let header = "# reinhardt-cloud.toml — Generated by `reinhardt-cloud init`\n\
		# All values are inferred from the reinhardt-web project.\n\
		# Edit to customize deployment configuration.\n\n";

	format!("{header}{output}")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::feature_detector::InfraSignals;
	use rstest::rstest;

	fn assert_default_runtime_sections(config: &ReinhardtCloudToml) {
		let health = config.health.as_ref().expect("health section");
		assert_eq!(health.path.as_deref(), Some(DEFAULT_HEALTH_PATH));
		assert_eq!(health.port, Some(DEFAULT_APP_PORT));
		assert_eq!(
			health.interval_seconds,
			Some(DEFAULT_HEALTH_INTERVAL_SECONDS)
		);

		let services = config.services.as_ref().expect("services section");
		assert_eq!(services.port, Some(DEFAULT_SERVICE_PORT));
		assert_eq!(services.target_port, Some(DEFAULT_APP_PORT));
		assert!(services.ingress_host.is_none());

		let scale = config.scale.as_ref().expect("scale section");
		assert_eq!(scale.min_replicas, Some(DEFAULT_MIN_REPLICAS));
		assert_eq!(scale.max_replicas, Some(DEFAULT_MAX_REPLICAS));
		assert_eq!(scale.metric.as_deref(), Some(DEFAULT_SCALE_METRIC));
		assert_eq!(scale.target_value, Some(DEFAULT_SCALE_TARGET_VALUE));
	}

	#[rstest]
	fn test_generate_config_with_database() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "my-app".into(),
			version: "0.1.0".into(),
			features: vec!["db-postgres".into(), "auth-jwt".into()],
			signals: InfraSignals {
				database: Some("postgresql".into()),
				jwt: true,
				..Default::default()
			},
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		assert_eq!(config.app.name, "my-app");
		assert_eq!(config.app.image, "my-app:latest");
		assert_eq!(config.database.as_ref().unwrap().engine, "postgresql");
		assert!(config.auth.as_ref().unwrap().jwt);
		assert_default_runtime_sections(&config);
		assert!(config.cache.is_none());
		assert!(config.worker.is_none());
	}

	#[rstest]
	fn test_generate_config_db_config_mysql_override_fails_early() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "app".into(),
			version: "0.1.0".into(),
			features: vec!["db-postgres".into()],
			signals: InfraSignals {
				database: Some("postgresql".into()),
				..Default::default()
			},
		};
		let db_config = DatabaseConfig {
			engine: "mysql".into(),
			host: Some("db.local".into()),
			port: Some(3306),
			name: "mydb".into(),
			user: Some("dbuser".into()),
		};

		// Act
		let error = generate_config(&metadata, Some(&db_config)).unwrap_err();

		// Assert
		assert!(error.contains("unsupported managed database engine"));
		assert!(error.contains("mysql"));
	}

	#[rstest]
	fn test_generate_config_minimal_no_infra() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "bare".into(),
			version: "0.1.0".into(),
			features: vec!["core".into()],
			signals: InfraSignals::default(),
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		assert_eq!(config.app.name, "bare");
		assert!(config.database.is_none());
		assert!(config.auth.is_none());
		assert!(config.cache.is_none());
		assert!(config.worker.is_none());
		assert!(config.storage.is_none());
		assert_default_runtime_sections(&config);
	}

	#[rstest]
	fn test_generate_config_with_cache_and_worker() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "full".into(),
			version: "0.1.0".into(),
			features: vec!["redis-backend".into(), "tasks".into()],
			signals: InfraSignals {
				cache: Some("redis".into()),
				background_worker: true,
				..Default::default()
			},
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		assert_eq!(config.cache.as_ref().unwrap().backend, "redis");
		assert!(config.worker.is_some());
	}

	#[rstest]
	fn test_generate_config_with_storage() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "storage-app".into(),
			version: "0.1.0".into(),
			features: vec!["storage".into()],
			signals: InfraSignals {
				object_storage: true,
				..Default::default()
			},
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		assert!(config.storage.is_some());
	}

	#[rstest]
	fn test_generate_config_derives_postgres_infrastructure() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "postgres-app".into(),
			version: "0.1.0".into(),
			features: vec!["db-postgres".into()],
			signals: InfraSignals {
				database: Some("postgresql".into()),
				..Default::default()
			},
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		let postgres = config
			.infrastructure
			.as_ref()
			.and_then(|infrastructure| infrastructure.postgres.as_ref())
			.expect("postgres infrastructure should be derived");
		assert_eq!(postgres.version.as_deref(), Some("16"));
		assert_eq!(postgres.backup_retention_days, Some(7));
		assert_eq!(postgres.tier, None);
	}

	#[rstest]
	fn test_generate_config_db_config_only_postgresql_derives_postgres_infrastructure() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "settings-db-app".into(),
			version: "0.1.0".into(),
			features: vec!["core".into()],
			signals: InfraSignals::default(),
		};
		let db_config = DatabaseConfig {
			engine: "postgresql".into(),
			host: Some("db.local".into()),
			port: Some(5432),
			name: "mydb".into(),
			user: Some("dbuser".into()),
		};

		// Act
		let config =
			generate_config(&metadata, Some(&db_config)).expect("config generation should succeed");

		// Assert
		assert_eq!(config.database.as_ref().unwrap().engine, "postgresql");
		assert!(
			config
				.infrastructure
				.as_ref()
				.and_then(|infrastructure| infrastructure.postgres.as_ref())
				.is_some()
		);
		assert_eq!(
			config
				.env
				.get("REINHARDT_DATABASE_HOST")
				.map(String::as_str),
			Some("db.local")
		);
		assert_eq!(
			config
				.env
				.get("REINHARDT_DATABASE_USER")
				.map(String::as_str),
			Some("dbuser")
		);
		assert!(!config.env.contains_key("DATABASE_URL"));
		assert!(!config.env.contains_key("REINHARDT_DATABASE_PASSWORD"));
	}

	#[rstest]
	fn test_generate_config_does_not_derive_bucket_from_boolean_storage_signal() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "storage-app".into(),
			version: "0.1.0".into(),
			features: vec!["storage".into()],
			signals: InfraSignals {
				object_storage: true,
				..Default::default()
			},
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		assert!(config.storage.is_some());
		assert!(
			config
				.infrastructure
				.as_ref()
				.and_then(|infrastructure| infrastructure.buckets.as_ref())
				.is_none()
		);
	}

	#[rstest]
	fn test_convert_infra_signals_maps_sessions_to_db_without_storage_backend() {
		// Arrange
		let signals = InfraSignals {
			object_storage: true,
			sessions: true,
			..Default::default()
		};

		// Act
		let converted = convert_infra_signals(&signals, None);

		// Assert
		assert_eq!(converted.session_backend.as_deref(), Some("db"));
		assert!(converted.storage.is_none());
	}

	#[rstest]
	fn test_generate_config_with_cache_enabled() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "cache-app".into(),
			version: "0.1.0".into(),
			features: vec!["redis-backend".into()],
			signals: InfraSignals {
				cache: Some("redis".into()),
				..Default::default()
			},
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		assert!(config.cache.is_some());
		assert_eq!(config.cache.as_ref().unwrap().backend, "redis");
	}

	#[rstest]
	fn test_generate_config_with_worker_enabled() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "worker-app".into(),
			version: "0.1.0".into(),
			features: vec!["tasks".into()],
			signals: InfraSignals {
				background_worker: true,
				..Default::default()
			},
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		assert!(config.worker.is_some());
	}

	#[rstest]
	fn test_generate_config_no_database_minimal_features() {
		// Arrange
		let metadata = ProjectMetadata {
			name: "no-db".into(),
			version: "0.1.0".into(),
			features: vec!["core".into(), "server".into()],
			signals: InfraSignals::default(),
		};

		// Act
		let config = generate_config(&metadata, None).expect("config generation should succeed");

		// Assert
		assert!(config.database.is_none());
		assert!(config.cache.is_none());
		assert!(config.worker.is_none());
		assert!(config.storage.is_none());
		assert!(config.auth.is_none());
		assert_eq!(config.app.name, "no-db");
	}

	#[rstest]
	fn test_generate_config_preserves_custom_env_vars() {
		// Arrange: generate_config does not populate env, but the
		// generated ReinhardtCloudToml env field defaults to empty BTreeMap.
		// Verify that manually setting env is preserved in serialization.
		let metadata = ProjectMetadata {
			name: "env-app".into(),
			version: "0.1.0".into(),
			features: vec![],
			signals: InfraSignals::default(),
		};

		// Act
		let mut config =
			generate_config(&metadata, None).expect("config generation should succeed");
		config
			.env
			.insert("MY_VAR".to_string(), "my_val".to_string());
		let output = generate_reinhardt_cloud_toml_string(&config);

		// Assert
		assert!(output.contains("MY_VAR"));
		assert!(output.contains("my_val"));
	}

	#[rstest]
	fn test_generate_reinhardt_cloud_toml_string_has_header() {
		// Arrange
		let config = ReinhardtCloudToml {
			app: AppSection {
				name: "test".into(),
				image: "test:latest".into(),
			},
			..Default::default()
		};

		// Act
		let output = generate_reinhardt_cloud_toml_string(&config);

		// Assert
		assert!(output.starts_with("# reinhardt-cloud.toml"));
		assert!(output.contains("reinhardt-cloud init"));
		assert!(output.contains("[app]"));
		assert!(output.contains("name = \"test\""));
	}

	#[rstest]
	fn test_generate_reinhardt_cloud_toml_string_roundtrip() {
		// Arrange
		let config = ReinhardtCloudToml {
			app: AppSection {
				name: "rt".into(),
				image: "rt:v1".into(),
			},
			database: Some(DatabaseSection {
				engine: "postgresql".into(),
				..Default::default()
			}),
			auth: Some(AuthSection { jwt: true }),
			health: Some(HealthSection {
				path: Some(DEFAULT_HEALTH_PATH.to_owned()),
				port: Some(DEFAULT_APP_PORT),
				interval_seconds: Some(DEFAULT_HEALTH_INTERVAL_SECONDS),
			}),
			services: Some(ServicesSection {
				port: Some(DEFAULT_SERVICE_PORT),
				target_port: Some(DEFAULT_APP_PORT),
				ingress_host: None,
			}),
			scale: Some(ScaleSection {
				min_replicas: Some(DEFAULT_MIN_REPLICAS),
				max_replicas: Some(DEFAULT_MAX_REPLICAS),
				metric: Some(DEFAULT_SCALE_METRIC.to_owned()),
				target_value: Some(DEFAULT_SCALE_TARGET_VALUE),
			}),
			..Default::default()
		};

		// Act
		let output = generate_reinhardt_cloud_toml_string(&config);
		// Strip header comments for parsing
		let toml_body: String = output
			.lines()
			.filter(|l| !l.starts_with('#'))
			.collect::<Vec<_>>()
			.join("\n");
		let parsed: ReinhardtCloudToml = toml::from_str(&toml_body).unwrap();

		// Assert
		assert_eq!(parsed.app.name, "rt");
		assert_eq!(
			parsed.database.as_ref().expect("database section").engine,
			"postgresql"
		);
		assert!(parsed.auth.as_ref().expect("auth section").jwt);
		assert_default_runtime_sections(&parsed);
	}
}
