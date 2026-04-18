//! Reads database configuration from `settings/base.toml`.

use std::path::Path;

/// Database configuration extracted from settings
#[derive(Debug, Clone, Default)]
// allow(dead_code) on host/port/name: parsed from settings/base.toml so future
// deploy-side manifest generation can surface them, but ReinhardtCloudToml's
// DatabaseSection currently exposes only `engine`. Keep the fields populated
// to avoid round-trip data loss when the schema gains host/port/name.
#[allow(dead_code)]
pub(crate) struct DatabaseConfig {
	pub(crate) engine: String,
	pub(crate) host: String,
	pub(crate) port: i32,
	pub(crate) name: String,
}

/// Parse database configuration from settings TOML content
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
		host: db
			.get("host")
			.and_then(|v| v.as_str())
			.unwrap_or("localhost")
			.to_owned(),
		port: db.get("port").and_then(|v| v.as_integer()).unwrap_or(5432) as i32,
		name: db
			.get("name")
			.and_then(|v| v.as_str())
			.unwrap_or("")
			.to_owned(),
	})
}

/// Read database config from settings directory
pub(crate) fn read_database_config(project_dir: &Path) -> Option<DatabaseConfig> {
	let base_toml = project_dir.join("settings").join("base.toml");
	let content = std::fs::read_to_string(base_toml).ok()?;
	parse_database_config(&content).ok()
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

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
		assert_eq!(config.host, "db.example.com");
		assert_eq!(config.port, 5432);
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
		assert_eq!(config.port, 3306);
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
		assert_eq!(config.host, "localhost");
		assert_eq!(config.port, 5432);
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
		let config = read_database_config(dir.path());

		// Assert
		let config = config.unwrap();
		assert_eq!(config.engine, "postgresql");
		assert_eq!(config.name, "test_db");
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
		assert_eq!(config.host, "mysql.example.com");
		assert_eq!(config.port, 3306);
		assert_eq!(config.name, "mysql_db");
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
