//! Infrastructure inference logic.
//!
//! Pure functions that interpret [`InfraSignals`] to determine
//! which Kubernetes resources need to be created.

use reinhardt_cloud_types::introspect::{InfraSignals, SettingsMetadata};

/// Check if the application requires a PostgreSQL database.
///
/// Matches both `"postgres"` and `"postgresql"` engine identifiers.
pub fn requires_postgresql(signals: &InfraSignals) -> bool {
	signals
		.database
		.as_deref()
		.is_some_and(|db| db == "postgres" || db == "postgresql")
}

/// Check if the application requires any database.
pub fn requires_database(signals: &InfraSignals) -> bool {
	signals.database.is_some()
}

/// Check if the application requires a cache (Redis, Memcached, etc.).
pub fn requires_cache(signals: &InfraSignals) -> bool {
	signals.cache.is_some()
}

/// Check if the application requires a background worker.
pub fn requires_worker(signals: &InfraSignals) -> bool {
	signals.background_worker
}

/// Check if the application requires WebSocket support.
pub fn requires_websocket(signals: &InfraSignals) -> bool {
	signals.websocket
}

/// Check if the application requires gRPC support.
pub fn requires_grpc(signals: &InfraSignals) -> bool {
	signals.grpc
}

/// Check if the application requires object storage.
pub fn requires_storage(signals: &InfraSignals) -> bool {
	signals.storage.is_some()
}

/// Check if the application requires a mail backend.
pub fn requires_mail(signals: &InfraSignals) -> bool {
	signals.mail.is_some()
}

/// Check if the application requires Redis for session storage.
pub fn requires_redis_sessions(signals: &InfraSignals) -> bool {
	signals
		.session_backend
		.as_deref()
		.is_some_and(|s| s == "redis")
}

/// Check if the application requires GraphQL support.
pub fn requires_graphql(signals: &InfraSignals) -> bool {
	signals.graphql
}

/// Check if the application requires an admin panel.
pub fn requires_admin(signals: &InfraSignals) -> bool {
	signals.admin_panel
}

/// Check if the application requires internationalization (i18n).
pub fn requires_i18n(signals: &InfraSignals) -> bool {
	signals.i18n
}

/// Get the application port from settings.
///
/// Returns the `default_port` from [`SettingsMetadata::server`],
/// which defaults to 8000 when not explicitly configured.
pub fn app_port(settings: &SettingsMetadata) -> u16 {
	settings.server.default_port
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::introspect::ServerSettings;
	use rstest::rstest;

	#[rstest]
	#[case(Some("postgres".to_string()), true)]
	#[case(Some("postgresql".to_string()), true)]
	#[case(Some("mysql".to_string()), false)]
	#[case(None, false)]
	fn test_requires_postgresql(#[case] database: Option<String>, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			database,
			..Default::default()
		};

		// Act
		let result = requires_postgresql(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(Some("postgres".to_string()), true)]
	#[case(Some("mysql".to_string()), true)]
	#[case(None, false)]
	fn test_requires_database(#[case] database: Option<String>, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			database,
			..Default::default()
		};

		// Act
		let result = requires_database(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(Some("redis".to_string()), true)]
	#[case(Some("memcached".to_string()), true)]
	#[case(None, false)]
	fn test_requires_cache(#[case] cache: Option<String>, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			cache,
			..Default::default()
		};

		// Act
		let result = requires_cache(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(true, true)]
	#[case(false, false)]
	fn test_requires_worker(#[case] background_worker: bool, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			background_worker,
			..Default::default()
		};

		// Act
		let result = requires_worker(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(true, true)]
	#[case(false, false)]
	fn test_requires_websocket(#[case] websocket: bool, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			websocket,
			..Default::default()
		};

		// Act
		let result = requires_websocket(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(true, true)]
	#[case(false, false)]
	fn test_requires_grpc(#[case] grpc: bool, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			grpc,
			..Default::default()
		};

		// Act
		let result = requires_grpc(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(8080, 8080)]
	#[case(3000, 3000)]
	fn test_app_port_custom(#[case] port: u16, #[case] expected: u16) {
		// Arrange
		let settings = SettingsMetadata {
			server: ServerSettings {
				default_port: port,
				..Default::default()
			},
			..Default::default()
		};

		// Act
		let result = app_port(&settings);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	fn test_app_port_default() {
		// Arrange
		let settings = SettingsMetadata::default();

		// Act
		let result = app_port(&settings);

		// Assert
		assert_eq!(result, 8000);
	}

	#[rstest]
	#[case(Some("s3".to_string()), true)]
	#[case(Some("gcs".to_string()), true)]
	#[case(None, false)]
	fn test_requires_storage(#[case] storage: Option<String>, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			storage,
			..Default::default()
		};

		// Act
		let result = requires_storage(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(Some("smtp".to_string()), true)]
	#[case(Some("ses".to_string()), true)]
	#[case(None, false)]
	fn test_requires_mail(#[case] mail: Option<String>, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			mail,
			..Default::default()
		};

		// Act
		let result = requires_mail(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(Some("redis".to_string()), true)]
	#[case(Some("db".to_string()), false)]
	#[case(None, false)]
	fn test_requires_redis_sessions(
		#[case] session_backend: Option<String>,
		#[case] expected: bool,
	) {
		// Arrange
		let signals = InfraSignals {
			session_backend,
			..Default::default()
		};

		// Act
		let result = requires_redis_sessions(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(true, true)]
	#[case(false, false)]
	fn test_requires_graphql(#[case] graphql: bool, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			graphql,
			..Default::default()
		};

		// Act
		let result = requires_graphql(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(true, true)]
	#[case(false, false)]
	fn test_requires_admin(#[case] admin_panel: bool, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			admin_panel,
			..Default::default()
		};

		// Act
		let result = requires_admin(&signals);

		// Assert
		assert_eq!(result, expected);
	}

	#[rstest]
	#[case(true, true)]
	#[case(false, false)]
	fn test_requires_i18n(#[case] i18n: bool, #[case] expected: bool) {
		// Arrange
		let signals = InfraSignals {
			i18n,
			..Default::default()
		};

		// Act
		let result = requires_i18n(&signals);

		// Assert
		assert_eq!(result, expected);
	}
}
