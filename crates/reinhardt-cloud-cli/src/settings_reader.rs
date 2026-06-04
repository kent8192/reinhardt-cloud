//! Reads database configuration from Reinhardt `settings/*.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use reinhardt::conf::settings::builder::SettingsBuilder;
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::sources::{DefaultSource, LowPriorityEnvSource, TomlFileSource};
use serde_json::Value;

/// Database configuration extracted from settings.
#[derive(Debug, Clone, Default)]
pub(crate) struct DatabaseConfig {
	pub(crate) engine: String,
	pub(crate) host: Option<String>,
	pub(crate) port: Option<u16>,
	pub(crate) name: String,
	pub(crate) user: Option<String>,
}

impl DatabaseConfig {
	fn fill_missing_from(&mut self, base: DatabaseConfig) {
		if self.engine.is_empty() {
			self.engine = base.engine;
		}
		if self.host.is_none() {
			self.host = base.host;
		}
		if self.port.is_none() {
			self.port = base.port;
		}
		if self.name.is_empty() {
			self.name = base.name;
		}
		if self.user.is_none() {
			self.user = base.user;
		}
	}

	/// Return non-secret deployment env vars derived from the settings DB block.
	pub(crate) fn deployment_env(&self) -> BTreeMap<String, String> {
		let mut env = BTreeMap::new();
		if let Some(host) = self.host.as_deref().filter(|host| !host.is_empty()) {
			env.insert("REINHARDT_DATABASE_HOST".to_string(), host.to_string());
		}
		if let Some(port) = self.port {
			env.insert("REINHARDT_DATABASE_PORT".to_string(), port.to_string());
		}
		if !self.name.is_empty() {
			env.insert("REINHARDT_DATABASE_NAME".to_string(), self.name.clone());
		}
		if let Some(user) = self.user.as_deref().filter(|user| !user.is_empty()) {
			env.insert("REINHARDT_DATABASE_USER".to_string(), user.to_string());
		}
		env
	}
}

/// Parse database configuration from a single settings TOML content string.
#[cfg(test)]
pub(crate) fn parse_database_config(content: &str) -> Result<DatabaseConfig, String> {
	let parsed: toml::Value =
		toml::from_str(content).map_err(|e| format!("Failed to parse settings: {e}"))?;

	let db = parsed
		.get("core")
		.and_then(|c| c.get("databases"))
		.and_then(|d| d.get("default"))
		.ok_or("No [core.databases.default] section found")?;

	Ok(DatabaseConfig {
		engine: db
			.get("engine")
			.and_then(|v| v.as_str())
			.unwrap_or("postgresql")
			.to_owned(),
		host: db.get("host").and_then(|v| v.as_str()).map(str::to_owned),
		port: db
			.get("port")
			.and_then(|v| v.as_integer())
			.and_then(|port| u16::try_from(port).ok()),
		name: db
			.get("name")
			.and_then(|v| v.as_str())
			.unwrap_or("")
			.to_owned(),
		user: db.get("user").and_then(|v| v.as_str()).map(str::to_owned),
	})
}

/// Read database config from a project settings directory.
pub(crate) fn read_database_config(project_dir: &Path) -> Option<DatabaseConfig> {
	let profile_name = std::env::var("REINHARDT_ENV").unwrap_or_else(|_| "production".to_string());
	read_database_config_for_profile(project_dir, &profile_name)
		.ok()
		.flatten()
}

fn read_database_config_for_profile(
	project_dir: &Path,
	profile_name: &str,
) -> Result<Option<DatabaseConfig>, String> {
	let settings_dir = project_dir.join("settings");
	let profile = Profile::parse(profile_name);

	let merged = SettingsBuilder::new()
		.profile(profile)
		.add_source(
			DefaultSource::new()
				.with_value("debug", Value::Bool(false))
				.with_value("language_code", Value::String("en-us".to_string()))
				.with_value("time_zone", Value::String("UTC".to_string())),
		)
		.add_source(LowPriorityEnvSource::new().with_prefix("REINHARDT_"))
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")).with_interpolation())
		.add_source(
			TomlFileSource::new(settings_dir.join(format!("{profile_name}.toml")))
				.with_interpolation(),
		)
		.build()
		.map_err(|e| format!("Failed to load settings: {e}"))?;

	let db_value = merged.get_raw("database").or_else(|| {
		merged
			.get_raw("core")
			.and_then(|core| core.get("databases"))
			.and_then(|databases| databases.get("default"))
	});

	let Some(mut config) = db_value.and_then(database_config_from_value) else {
		return Ok(None);
	};

	if profile_name != "base"
		&& let Some(base_config) = read_database_config_for_profile(project_dir, "base")?
	{
		config.fill_missing_from(base_config);
	}

	Ok(Some(config))
}

fn database_config_from_value(value: &Value) -> Option<DatabaseConfig> {
	serde_json::from_value::<reinhardt::conf::settings::DatabaseConfig>(value.clone())
		.ok()
		.map(database_config_from_reinhardt)
		.or_else(|| database_config_from_object(value))
}

fn database_config_from_reinhardt(
	config: reinhardt::conf::settings::DatabaseConfig,
) -> DatabaseConfig {
	DatabaseConfig {
		engine: config.engine,
		host: config.host,
		port: config.port,
		name: config.name,
		user: config.user,
	}
}

fn database_config_from_object(value: &Value) -> Option<DatabaseConfig> {
	let Value::Object(map) = value else {
		return None;
	};

	Some(DatabaseConfig {
		engine: map
			.get("engine")
			.and_then(Value::as_str)
			.unwrap_or("sqlite")
			.to_string(),
		host: map.get("host").and_then(Value::as_str).map(str::to_owned),
		port: map
			.get("port")
			.and_then(Value::as_u64)
			.and_then(|port| u16::try_from(port).ok()),
		name: map
			.get("name")
			.and_then(Value::as_str)
			.unwrap_or("db.sqlite3")
			.to_string(),
		user: map.get("user").and_then(Value::as_str).map(str::to_owned),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use serial_test::serial;

	#[rstest]
	fn test_parse_database_config_postgresql() {
		// Arrange
		let content = r#"
[core.databases.default]
engine = "postgresql"
host = "db.example.com"
port = 5432
name = "mydb"
"#;

		// Act
		let config = parse_database_config(content).unwrap();

		// Assert
		assert_eq!(config.engine, "postgresql");
		assert_eq!(config.host.as_deref(), Some("db.example.com"));
		assert_eq!(config.port, Some(5432));
		assert_eq!(config.name, "mydb");
	}

	#[rstest]
	fn test_parse_database_config_mysql() {
		// Arrange
		let content = r#"
[core.databases.default]
engine = "mysql"
host = "mysql.local"
port = 3306
name = "app_db"
"#;

		// Act
		let config = parse_database_config(content).unwrap();

		// Assert
		assert_eq!(config.engine, "mysql");
		assert_eq!(config.port, Some(3306));
	}

	#[rstest]
	fn test_parse_database_config_defaults() {
		// Arrange
		let content = r#"
[core.databases.default]
"#;

		// Act
		let config = parse_database_config(content).unwrap();

		// Assert
		assert_eq!(config.engine, "postgresql");
		assert_eq!(config.host, None);
		assert_eq!(config.port, None);
		assert_eq!(config.name, "");
	}

	#[rstest]
	fn test_parse_database_config_missing_section() {
		// Arrange
		let content = r#"
[server]
host = "localhost"
"#;

		// Act
		let result = parse_database_config(content);

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("No [core.databases.default]"));
	}

	#[rstest]
	fn test_parse_database_config_invalid_toml() {
		// Arrange
		let content = "not valid toml {{{";

		// Act
		let result = parse_database_config(content);

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("Failed to parse settings"));
	}

	#[rstest]
	fn test_read_database_config_from_dir() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let settings_dir = dir.path().join("settings");
		std::fs::create_dir(&settings_dir).unwrap();
		std::fs::write(
			settings_dir.join("base.toml"),
			r#"
[core.databases.default]
engine = "postgresql"
host = "localhost"
port = 5432
name = "test_db"
"#,
		)
		.unwrap();

		// Act
		let config = read_database_config_for_profile(dir.path(), "production")
			.unwrap()
			.unwrap();

		// Assert
		assert_eq!(config.engine, "postgresql");
		assert_eq!(config.name, "test_db");
	}

	#[rstest]
	#[serial(settings_env)]
	fn test_read_database_config_merges_profile_with_interpolation() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let settings_dir = dir.path().join("settings");
		std::fs::create_dir(&settings_dir).unwrap();
		std::fs::write(
			settings_dir.join("base.toml"),
			r#"
[core.databases.default]
engine = "postgresql"
host = "base-db.internal"
port = 5432
name = "base_db"
user = "base_user"
password = "do-not-export"
"#,
		)
		.unwrap();
		std::fs::write(
			settings_dir.join("production.toml"),
			r#"
[core.databases.default]
host = "${REINHARDT_DATABASE_HOST:-cloudsql.internal}"
name = "prod_db"
"#,
		)
		.unwrap();
		let original_host = std::env::var("REINHARDT_DATABASE_HOST").ok();
		unsafe { std::env::remove_var("REINHARDT_DATABASE_HOST") };

		// Act
		let config = read_database_config_for_profile(dir.path(), "production")
			.unwrap()
			.unwrap();
		let env = config.deployment_env();

		// Assert
		assert_eq!(config.host.as_deref(), Some("cloudsql.internal"));
		assert_eq!(config.port, Some(5432));
		assert_eq!(config.name, "prod_db");
		assert_eq!(config.user.as_deref(), Some("base_user"));
		assert_eq!(
			env.get("REINHARDT_DATABASE_HOST").map(String::as_str),
			Some("cloudsql.internal")
		);
		assert!(!env.contains_key("REINHARDT_DATABASE_PASSWORD"));
		assert!(!env.contains_key("DATABASE_URL"));

		// Cleanup
		match original_host {
			Some(value) => unsafe { std::env::set_var("REINHARDT_DATABASE_HOST", value) },
			None => unsafe { std::env::remove_var("REINHARDT_DATABASE_HOST") },
		}
	}

	#[rstest]
	fn test_parse_database_config_missing_databases_section_entirely() {
		// Arrange
		let content = r#"
[app]
name = "test"
"#;

		// Act
		let result = parse_database_config(content);

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("No [core.databases.default]"));
	}

	#[rstest]
	fn test_parse_database_config_invalid_toml_content() {
		// Arrange
		let content = "this is { not } valid toml [[";

		// Act
		let result = parse_database_config(content);

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("Failed to parse settings"));
	}

	#[rstest]
	fn test_parse_database_config_mysql_engine_detection() {
		// Arrange
		let content = r#"
[core.databases.default]
engine = "mysql"
host = "mysql.example.com"
port = 3306
name = "mysql_db"
"#;

		// Act
		let config = parse_database_config(content).unwrap();

		// Assert
		assert_eq!(config.engine, "mysql");
		assert_eq!(config.host.as_deref(), Some("mysql.example.com"));
		assert_eq!(config.port, Some(3306));
	}

	#[rstest]
	fn test_read_database_config_no_settings_dir() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();

		// Act
		let config = read_database_config(dir.path());

		// Assert
		assert!(config.is_none());
	}
}
