//! Environment variable auto-injection and merging for deployed applications.
//!
//! Provides builders for database connection strings, system variables, and
//! a priority-based merge function where user overrides always win.

use std::collections::{BTreeMap, HashSet};

use k8s_openapi::api::core::v1::EnvVar;

/// Build database connection environment variables from raw credentials.
///
/// Generates both a composite `DATABASE_URL` (for frameworks that expect a
/// single connection string) and individual `REINHARDT_DATABASE_*` variables.
pub(crate) fn build_database_env_vars(
	endpoint: &str,
	port: i32,
	db_name: &str,
	user: &str,
	password: &str,
) -> Vec<EnvVar> {
	let database_url = format!("postgresql://{user}:{password}@{endpoint}:{port}/{db_name}");
	vec![
		env_var("DATABASE_URL", &database_url),
		env_var("REINHARDT_DATABASE_HOST", endpoint),
		env_var("REINHARDT_DATABASE_PORT", &port.to_string()),
		env_var("REINHARDT_DATABASE_NAME", db_name),
		env_var("REINHARDT_DATABASE_USER", user),
		env_var("REINHARDT_DATABASE_PASSWORD", password),
	]
}

/// Build system environment variables that are always injected.
pub(crate) fn build_system_env_vars() -> Vec<EnvVar> {
	vec![
		env_var("REINHARDT_ENV", "production"),
		env_var("NUAGES_CONFIG_DIR", "/etc/nuages/settings"),
	]
}

/// Merge auto-generated and user-supplied environment variables.
///
/// User overrides (`user_vars`) always take priority over auto-generated
/// variables (`auto_vars`). When both define the same key, the user value
/// is kept and the auto-generated value is discarded.
pub(crate) fn merge_env_vars(
	auto_vars: &[EnvVar],
	user_vars: &BTreeMap<String, String>,
) -> Vec<EnvVar> {
	let mut result: Vec<EnvVar> = Vec::new();
	let mut seen = HashSet::new();

	// User vars first (highest priority)
	for (k, v) in user_vars {
		result.push(env_var(k, v));
		seen.insert(k.clone());
	}

	// Auto vars only if not overridden by user
	for var in auto_vars {
		if !seen.contains(&var.name) {
			result.push(var.clone());
			seen.insert(var.name.clone());
		}
	}

	result
}

fn env_var(name: &str, value: &str) -> EnvVar {
	EnvVar {
		name: name.to_string(),
		value: Some(value.to_string()),
		..Default::default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn build_database_env_vars_generates_all_keys() {
		// Arrange
		let endpoint = "db.example.com";
		let port = 5432;
		let db_name = "mydb";
		let user = "admin";
		let password = "secret";

		// Act
		let vars = build_database_env_vars(endpoint, port, db_name, user, password);

		// Assert
		assert_eq!(vars.len(), 6);

		let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
		assert!(names.contains(&"DATABASE_URL"));
		assert!(names.contains(&"REINHARDT_DATABASE_HOST"));
		assert!(names.contains(&"REINHARDT_DATABASE_PORT"));
		assert!(names.contains(&"REINHARDT_DATABASE_NAME"));
		assert!(names.contains(&"REINHARDT_DATABASE_USER"));
		assert!(names.contains(&"REINHARDT_DATABASE_PASSWORD"));
	}

	#[rstest]
	fn build_database_env_vars_constructs_correct_url() {
		// Arrange & Act
		let vars = build_database_env_vars("host.local", 5432, "testdb", "user1", "pass1");

		// Assert
		let url_var = vars.iter().find(|v| v.name == "DATABASE_URL").unwrap();
		assert_eq!(
			url_var.value.as_deref(),
			Some("postgresql://user1:pass1@host.local:5432/testdb")
		);
	}

	#[rstest]
	fn build_database_env_vars_sets_individual_fields() {
		// Arrange & Act
		let vars = build_database_env_vars("myhost", 3306, "mydb", "root", "pw");

		// Assert
		let host_var = vars
			.iter()
			.find(|v| v.name == "REINHARDT_DATABASE_HOST")
			.unwrap();
		assert_eq!(host_var.value.as_deref(), Some("myhost"));

		let port_var = vars
			.iter()
			.find(|v| v.name == "REINHARDT_DATABASE_PORT")
			.unwrap();
		assert_eq!(port_var.value.as_deref(), Some("3306"));
	}

	#[rstest]
	fn build_system_env_vars_contains_required_keys() {
		// Arrange & Act
		let vars = build_system_env_vars();

		// Assert
		assert_eq!(vars.len(), 2);

		let env_var = vars.iter().find(|v| v.name == "REINHARDT_ENV").unwrap();
		assert_eq!(env_var.value.as_deref(), Some("production"));

		let config_var = vars.iter().find(|v| v.name == "NUAGES_CONFIG_DIR").unwrap();
		assert_eq!(config_var.value.as_deref(), Some("/etc/nuages/settings"));
	}

	#[rstest]
	fn merge_env_vars_user_overrides_auto_vars() {
		// Arrange
		let auto_vars = vec![
			env_var("DATABASE_URL", "auto-url"),
			env_var("REINHARDT_ENV", "production"),
		];
		let user_vars = BTreeMap::from([("DATABASE_URL".to_string(), "custom-url".to_string())]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		let db_var = merged.iter().find(|v| v.name == "DATABASE_URL").unwrap();
		assert_eq!(db_var.value.as_deref(), Some("custom-url"));

		// Auto var not overridden is preserved
		let env_var = merged.iter().find(|v| v.name == "REINHARDT_ENV").unwrap();
		assert_eq!(env_var.value.as_deref(), Some("production"));
	}

	#[rstest]
	fn merge_env_vars_preserves_all_unique_keys() {
		// Arrange
		let auto_vars = vec![env_var("AUTO_KEY", "auto_val")];
		let user_vars = BTreeMap::from([("USER_KEY".to_string(), "user_val".to_string())]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 2);
		assert!(merged.iter().any(|v| v.name == "AUTO_KEY"));
		assert!(merged.iter().any(|v| v.name == "USER_KEY"));
	}

	#[rstest]
	fn merge_env_vars_no_duplicates() {
		// Arrange
		let auto_vars = vec![env_var("KEY_A", "auto_a"), env_var("KEY_B", "auto_b")];
		let user_vars = BTreeMap::from([
			("KEY_A".to_string(), "user_a".to_string()),
			("KEY_C".to_string(), "user_c".to_string()),
		]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 3);
		let key_a_count = merged.iter().filter(|v| v.name == "KEY_A").count();
		assert_eq!(key_a_count, 1);
	}

	#[rstest]
	fn merge_env_vars_empty_user_vars_returns_auto() {
		// Arrange
		let auto_vars = vec![env_var("A", "1"), env_var("B", "2")];
		let user_vars = BTreeMap::new();

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 2);
	}

	#[rstest]
	fn merge_env_vars_empty_auto_vars_returns_user() {
		// Arrange
		let auto_vars: Vec<EnvVar> = vec![];
		let user_vars = BTreeMap::from([("X".to_string(), "y".to_string())]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 1);
		assert_eq!(merged[0].name, "X");
	}
}
