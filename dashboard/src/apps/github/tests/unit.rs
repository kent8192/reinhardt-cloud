//! Unit tests for GitHub App integration.

#[cfg(test)]
pub mod pipeline_tests {
	use std::time::Duration;

	use rstest::rstest;

	use crate::apps::github::services::pipeline::{
		credentials_secret_for_repository, github_credentials_secret_name, installation_clone_url,
		parse_introspect_timeout_seconds, redacted_clone_url,
	};

	#[rstest]
	fn test_installation_clone_url_percent_encodes_token_and_redacts_display() {
		// Arrange
		let full_name = "kent8192/private-app";
		let token = "ghs_token:with/slash";

		// Act
		let clone_url = installation_clone_url(full_name, token);
		let redacted = redacted_clone_url(full_name);

		// Assert
		assert_eq!(
			clone_url,
			"https://x-access-token:ghs%5Ftoken%3Awith%2Fslash@github.com/kent8192/private-app.git"
		);
		assert_eq!(
			redacted,
			"https://x-access-token:[redacted]@github.com/kent8192/private-app.git"
		);
		assert!(!redacted.contains(token));
	}

	#[rstest]
	#[case::positive("30", Some(Duration::from_secs(30)))]
	#[case::zero_disables_timeout("0", None)]
	#[case::invalid("abc", None)]
	fn test_parse_introspect_timeout_seconds(
		#[case] raw: &str,
		#[case] expected: Option<Duration>,
	) {
		// Arrange / Act / Assert
		assert_eq!(parse_introspect_timeout_seconds(raw), expected);
	}

	#[rstest]
	fn test_credentials_secret_for_private_repository_only() {
		// Arrange / Act / Assert
		assert_eq!(
			github_credentials_secret_name("private-app"),
			"private-app-github-git-credentials"
		);
		assert_eq!(
			credentials_secret_for_repository("private-app", true).as_deref(),
			Some("private-app-github-git-credentials")
		);
		assert!(credentials_secret_for_repository("public-app", false).is_none());
	}
}

#[cfg(test)]
pub mod client_tests {
	use reqwest::Client;
	use rstest::rstest;
	use serde_json::json;
	use wiremock::matchers::{header, method, path, query_param};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	use crate::apps::github::services::client::{
		ReqwestGitHubAppClient, installation_access_tokens_url, installation_repositories_url,
	};
	use crate::apps::github::services::config::GitHubAppSettings;

	fn test_settings(api_base_url: String) -> GitHubAppSettings {
		GitHubAppSettings {
			app_id: 12_345,
			private_key_pem: "unused-private-key".to_string(),
			webhook_secret: "unused-webhook-secret".to_string(),
			api_base_url,
		}
	}

	fn repository_json(
		id: i64,
		owner_login: &str,
		full_name: &str,
		name: &str,
		private: bool,
		default_branch: &str,
	) -> serde_json::Value {
		json!({
			"id": id,
			"full_name": full_name,
			"name": name,
			"private": private,
			"default_branch": default_branch,
			"owner": {
				"login": owner_login,
			},
		})
	}

	#[rstest]
	fn test_github_app_client_builds_installation_urls_with_base_path() {
		// Arrange
		let api_base_url = "https://github.example.test/api/v3/";

		// Act
		let access_token_url =
			installation_access_tokens_url(api_base_url, 987_654).expect("url should build");
		let repositories_url =
			installation_repositories_url(api_base_url).expect("url should build");

		// Assert
		assert_eq!(
			access_token_url.as_str(),
			"https://github.example.test/api/v3/app/installations/987654/access_tokens"
		);
		assert_eq!(
			repositories_url.as_str(),
			"https://github.example.test/api/v3/installation/repositories"
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_github_app_client_lists_repositories_with_headers_and_pagination() {
		// Arrange
		let server = MockServer::start().await;
		let mut first_page_repositories = vec![repository_json(
			1,
			"kent8192",
			"kent8192/reinhardt-cloud",
			"reinhardt-cloud",
			true,
			"main",
		)];
		for id in 2..=100 {
			first_page_repositories.push(repository_json(
				id,
				"kent8192",
				&format!("kent8192/repo-{id}"),
				&format!("repo-{id}"),
				false,
				"trunk",
			));
		}
		let first_page = json!({
			"total_count": 101,
			"repositories": first_page_repositories,
		});
		let second_page_repositories = vec![repository_json(
			101,
			"kent8192",
			"kent8192/repo-101",
			"repo-101",
			false,
			"trunk",
		)];
		let second_page = json!({
			"total_count": 101,
			"repositories": second_page_repositories,
		});
		Mock::given(method("GET"))
			.and(path("/installation/repositories"))
			.and(query_param("per_page", "100"))
			.and(query_param("page", "1"))
			.and(header("authorization", "Bearer installation-token"))
			.and(header("accept", "application/vnd.github+json"))
			.and(header("user-agent", "reinhardt-cloud-dashboard"))
			.and(header("x-github-api-version", "2022-11-28"))
			.respond_with(ResponseTemplate::new(200).set_body_json(first_page))
			.expect(1)
			.mount(&server)
			.await;
		Mock::given(method("GET"))
			.and(path("/installation/repositories"))
			.and(query_param("per_page", "100"))
			.and(query_param("page", "2"))
			.and(header("authorization", "Bearer installation-token"))
			.and(header("accept", "application/vnd.github+json"))
			.and(header("user-agent", "reinhardt-cloud-dashboard"))
			.and(header("x-github-api-version", "2022-11-28"))
			.respond_with(ResponseTemplate::new(200).set_body_json(second_page))
			.expect(1)
			.mount(&server)
			.await;
		let client = ReqwestGitHubAppClient::with_http_client(
			test_settings(server.uri()),
			Client::builder()
				.build()
				.expect("reqwest client should build"),
		);

		// Act
		let repositories = client
			.list_repositories_with_token("installation-token")
			.await
			.expect("repositories should list");

		// Assert
		assert_eq!(repositories.len(), 101);
		assert_eq!(repositories[0].owner_login, "kent8192");
		assert_eq!(repositories[0].id, 1);
		assert_eq!(repositories[0].full_name, "kent8192/reinhardt-cloud");
		assert_eq!(repositories[0].name, "reinhardt-cloud");
		assert!(repositories[0].private);
		assert_eq!(repositories[0].default_branch, "main");
		assert_eq!(repositories[100].full_name, "kent8192/repo-101");
		assert!(!repositories[100].private);
		assert_eq!(repositories[100].default_branch, "trunk");
	}
}

#[cfg(test)]
pub mod import_tests {
	use reinhardt_cloud_types::introspect::{AppMetadata, IntrospectOutput};
	use rstest::rstest;

	use crate::apps::github::models::GitHubRepository;
	use crate::apps::github::services::import::{
		apply_webhook_action_to_manifest, enrich_import_spec, import_spec_from_repository,
		source_reinhardt_app_yaml, validate_app_name, validate_registry, with_build_trigger,
		with_preview_create, with_preview_delete,
	};
	use crate::utils::vcs::events::WebhookAction;

	fn repository(name: &str) -> GitHubRepository {
		GitHubRepository::build()
			.installation(7)
			.github_repository_id(123_456_789)
			.full_name(format!("kent8192/{name}"))
			.owner_login("kent8192".to_string())
			.name(name.to_string())
			.default_branch("main".to_string())
			.private(true)
			.selected(false)
			.finish()
	}

	#[rstest]
	#[case("reinhardt-cloud", "reinhardt-cloud")]
	#[case("Reinhardt.Cloud_App", "reinhardt-cloud-app")]
	#[case("___", "app")]
	fn test_import_spec_derives_valid_default_app_name(
		#[case] repo_name: &str,
		#[case] expected_app_name: &str,
	) {
		// Arrange
		let repository = repository(repo_name);

		// Act
		let spec = import_spec_from_repository(&repository, "", "ghcr.io/kent8192/app")
			.expect("import spec should build");

		// Assert
		assert_eq!(spec.app_name, expected_app_name);
		assert_eq!(
			spec.repository_url,
			format!("https://github.com/kent8192/{repo_name}.git")
		);
		assert_eq!(spec.branch, "main");
		assert_eq!(spec.registry, "ghcr.io/kent8192/app");
	}

	#[rstest]
	#[case("")]
	#[case("-bad")]
	#[case("bad-")]
	#[case("Bad")]
	#[case("bad_name")]
	fn test_validate_app_name_rejects_invalid_names(#[case] name: &str) {
		// Arrange / Act
		let result = validate_app_name(name);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_validate_registry_rejects_whitespace() {
		// Arrange / Act
		let result = validate_registry("ghcr.io/example/app latest");

		// Assert
		assert_eq!(
			result.expect_err("registry with whitespace should fail"),
			"Registry image prefix must not contain whitespace"
		);
	}

	#[rstest]
	fn test_source_manifest_contains_github_source_and_initial_build_trigger() {
		// Arrange
		let repository = repository("reinhardt-cloud");
		let spec = import_spec_from_repository(&repository, "", "ghcr.io/kent8192/reinhardt-cloud")
			.expect("import spec should build");

		// Act
		let yaml = source_reinhardt_app_yaml(&spec).expect("manifest should serialize");
		let value: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("yaml should parse");

		// Assert
		assert_eq!(value["kind"].as_str(), Some("Project"));
		assert_eq!(value["metadata"]["name"].as_str(), Some("reinhardt-cloud"));
		assert_eq!(
			value["metadata"]["annotations"]["reinhardt.dev/build-trigger"].as_str(),
			Some("initial")
		);
		assert_eq!(
			value["spec"]["source"]["repository"].as_str(),
			Some("https://github.com/kent8192/reinhardt-cloud.git")
		);
		assert_eq!(value["spec"]["source"]["branch"].as_str(), Some("main"));
		assert_eq!(value["spec"]["source"]["provider"].as_str(), Some("github"));
		assert_eq!(
			value["spec"]["source"]["webhook"]["enabled"].as_bool(),
			Some(true)
		);
		assert_eq!(
			value["spec"]["source"]["preview"]["enabled"].as_bool(),
			Some(true)
		);
	}

	#[rstest]
	fn test_source_manifest_contains_introspect_and_credentials_secret() {
		// Arrange
		let repository = repository("private-app");
		let mut spec = import_spec_from_repository(&repository, "", "ghcr.io/kent8192/private-app")
			.expect("import spec should build");
		let introspect = IntrospectOutput {
			app: AppMetadata {
				name: "private-app".to_string(),
				version: "0.1.0".to_string(),
			},
			..Default::default()
		};
		enrich_import_spec(
			&mut spec,
			introspect,
			Some("private-app-github-git-credentials".to_string()),
		);

		// Act
		let yaml = source_reinhardt_app_yaml(&spec).expect("manifest should serialize");
		let value: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("yaml should parse");

		// Assert
		assert_eq!(
			value["spec"]["introspect"]["app"]["name"].as_str(),
			Some("private-app")
		);
		assert_eq!(
			value["spec"]["introspect"]["app"]["version"].as_str(),
			Some("0.1.0")
		);
		assert_eq!(
			value["spec"]["source"]["credentials_secret"].as_str(),
			Some("private-app-github-git-credentials")
		);
	}

	#[rstest]
	fn test_webhook_helpers_update_project_annotations() {
		// Arrange
		let repository = repository("reinhardt-cloud");
		let spec = import_spec_from_repository(&repository, "", "ghcr.io/kent8192/reinhardt-cloud")
			.expect("import spec should build");
		let yaml = source_reinhardt_app_yaml(&spec).expect("manifest should serialize");

		// Act
		let pushed = with_build_trigger(&yaml, "abc123").expect("push annotation should apply");
		let preview = with_preview_create(&pushed, 42, "feature/login", "def456")
			.expect("preview annotation should apply");
		let deleted = with_preview_delete(&preview, 42).expect("preview delete should apply");
		let preview_value: serde_yaml::Value =
			serde_yaml::from_str(&preview).expect("preview yaml should parse");
		let deleted_value: serde_yaml::Value =
			serde_yaml::from_str(&deleted).expect("delete yaml should parse");

		// Assert
		assert_eq!(
			preview_value["metadata"]["annotations"]["reinhardt.dev/preview-action"].as_str(),
			Some("create")
		);
		assert_eq!(
			preview_value["metadata"]["annotations"]["reinhardt.dev/pr-number"].as_str(),
			Some("42")
		);
		assert_eq!(
			preview_value["metadata"]["annotations"]["reinhardt.dev/pr-branch"].as_str(),
			Some("feature/login")
		);
		assert_eq!(
			preview_value["metadata"]["annotations"]["reinhardt.dev/build-trigger"].as_str(),
			Some("def456")
		);
		assert_eq!(
			deleted_value["metadata"]["annotations"]["reinhardt.dev/preview-action"].as_str(),
			Some("delete")
		);
		assert!(deleted_value["metadata"]["annotations"]["reinhardt.dev/pr-branch"].is_null());
	}

	#[rstest]
	fn test_webhook_action_applies_production_push_only_for_tracked_branch() {
		// Arrange
		let repository = repository("reinhardt-cloud");
		let spec = import_spec_from_repository(&repository, "", "ghcr.io/kent8192/reinhardt-cloud")
			.expect("import spec should build");
		let yaml = source_reinhardt_app_yaml(&spec).expect("manifest should serialize");
		let production_push = WebhookAction::BuildTrigger {
			branch: "main".to_string(),
			commit_sha: "abc123".to_string(),
		};
		let feature_push = WebhookAction::BuildTrigger {
			branch: "feature/login".to_string(),
			commit_sha: "def456".to_string(),
		};

		// Act
		let updated = apply_webhook_action_to_manifest(&yaml, &production_push, "main")
			.expect("production push should be valid")
			.expect("production push should update manifest");
		let ignored = apply_webhook_action_to_manifest(&yaml, &feature_push, "main")
			.expect("feature push should be valid");
		let value: serde_yaml::Value =
			serde_yaml::from_str(&updated).expect("updated yaml should parse");

		// Assert
		assert_eq!(ignored, None);
		assert_eq!(
			value["metadata"]["annotations"]["reinhardt.dev/build-trigger"].as_str(),
			Some("abc123")
		);
	}

	#[rstest]
	fn test_webhook_action_applies_preview_lifecycle_annotations() {
		// Arrange
		let repository = repository("reinhardt-cloud");
		let spec = import_spec_from_repository(&repository, "", "ghcr.io/kent8192/reinhardt-cloud")
			.expect("import spec should build");
		let yaml = source_reinhardt_app_yaml(&spec).expect("manifest should serialize");
		let create = WebhookAction::PreviewCreate {
			pr_number: 42,
			branch: "feature/login".to_string(),
			commit_sha: "abc123".to_string(),
		};
		let delete = WebhookAction::PreviewDelete { pr_number: 42 };

		// Act
		let created = apply_webhook_action_to_manifest(&yaml, &create, "main")
			.expect("preview create should be valid")
			.expect("preview create should update manifest");
		let deleted = apply_webhook_action_to_manifest(&created, &delete, "main")
			.expect("preview delete should be valid")
			.expect("preview delete should update manifest");
		let created_value: serde_yaml::Value =
			serde_yaml::from_str(&created).expect("created yaml should parse");
		let deleted_value: serde_yaml::Value =
			serde_yaml::from_str(&deleted).expect("deleted yaml should parse");

		// Assert
		assert_eq!(
			created_value["metadata"]["annotations"]["reinhardt.dev/preview-action"].as_str(),
			Some("create")
		);
		assert_eq!(
			deleted_value["metadata"]["annotations"]["reinhardt.dev/preview-action"].as_str(),
			Some("delete")
		);
		assert!(deleted_value["metadata"]["annotations"]["reinhardt.dev/build-trigger"].is_null());
	}
}

#[cfg(test)]
pub mod webhook_tests {
	use rstest::rstest;
	use serde_json::json;

	use crate::apps::github::services::webhook::parse_github_webhook_dispatch;
	use crate::utils::vcs::events::WebhookAction;

	#[rstest]
	fn test_github_webhook_dispatch_reads_repository_id_and_action() {
		// Arrange
		let payload = json!({
			"repository": { "id": 123456789 },
			"ref": "refs/heads/main",
			"after": "abc123"
		});
		let bytes = serde_json::to_vec(&payload).expect("payload should serialize");

		// Act
		let dispatch =
			parse_github_webhook_dispatch("push", &bytes).expect("dispatch should parse");

		// Assert
		assert_eq!(dispatch.repository_id, 123_456_789);
		assert_eq!(
			dispatch.action,
			WebhookAction::BuildTrigger {
				branch: "main".to_string(),
				commit_sha: "abc123".to_string(),
			}
		);
	}

	#[rstest]
	fn test_github_webhook_dispatch_rejects_payload_without_repository_id() {
		// Arrange
		let payload = json!({
			"ref": "refs/heads/main",
			"after": "abc123"
		});
		let bytes = serde_json::to_vec(&payload).expect("payload should serialize");

		// Act
		let result = parse_github_webhook_dispatch("push", &bytes);

		// Assert
		assert!(result.is_err());
	}
}

#[cfg(test)]
pub mod config_tests {
	use rstest::rstest;
	use serial_test::serial;

	use crate::apps::github::services::config::GitHubAppSettings;

	const APP_ID_ENV: &str = "REINHARDT_CLOUD_GITHUB_APP_ID";
	const PRIVATE_KEY_PEM_ENV: &str = "REINHARDT_CLOUD_GITHUB_APP_PRIVATE_KEY_PEM";
	const WEBHOOK_SECRET_ENV: &str = "REINHARDT_CLOUD_GITHUB_WEBHOOK_SECRET";
	const API_BASE_URL_ENV: &str = "REINHARDT_CLOUD_GITHUB_API_BASE_URL";

	struct EnvGuard {
		saved: Vec<(String, Option<String>)>,
	}

	impl EnvGuard {
		fn set(vars: Vec<(&str, Option<&str>)>) -> Self {
			let mut saved = Vec::new();
			for (key, value) in &vars {
				saved.push((key.to_string(), std::env::var(key).ok()));
				// SAFETY: these tests are serialized and mutate env vars before the act phase.
				unsafe {
					match value {
						Some(value) => std::env::set_var(key, value),
						None => std::env::remove_var(key),
					}
				}
			}
			Self { saved }
		}
	}

	impl Drop for EnvGuard {
		fn drop(&mut self) {
			for (key, value) in &self.saved {
				// SAFETY: these tests are serialized and restore env vars during teardown.
				unsafe {
					match value {
						Some(value) => std::env::set_var(key, value),
						None => std::env::remove_var(key),
					}
				}
			}
		}
	}

	#[rstest]
	#[serial(env_github_app_settings)]
	fn test_github_app_settings_loads_required_env() {
		// Arrange
		let _env = EnvGuard::set(vec![
			(APP_ID_ENV, Some("12345")),
			(
				PRIVATE_KEY_PEM_ENV,
				Some("-----BEGIN PRIVATE KEY-----\\nabc\\n-----END PRIVATE KEY-----"),
			),
			(WEBHOOK_SECRET_ENV, Some("webhook-secret")),
			(API_BASE_URL_ENV, None),
		]);

		// Act
		let settings =
			GitHubAppSettings::from_env().expect("required GitHub App settings should load");

		// Assert
		assert_eq!(settings.app_id, 12345);
		assert_eq!(
			settings.private_key_pem,
			"-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----"
		);
		assert_eq!(settings.webhook_secret, "webhook-secret");
		assert_eq!(settings.api_base_url, "https://api.github.com");
	}

	#[rstest]
	#[serial(env_github_app_settings)]
	fn test_github_app_settings_rejects_missing_private_key() {
		// Arrange
		let _env = EnvGuard::set(vec![
			(APP_ID_ENV, Some("12345")),
			(PRIVATE_KEY_PEM_ENV, Some("   ")),
			(WEBHOOK_SECRET_ENV, Some("webhook-secret")),
			(API_BASE_URL_ENV, Some("https://github.example.test/api/v3")),
		]);

		// Act
		let err = GitHubAppSettings::from_env().expect_err("blank private key should be rejected");

		// Assert
		assert_eq!(
			err.to_string(),
			"REINHARDT_CLOUD_GITHUB_APP_PRIVATE_KEY_PEM is required"
		);
	}

	#[rstest]
	#[serial(env_github_app_settings)]
	fn test_github_app_settings_rejects_escaped_blank_private_key() {
		// Arrange
		let _env = EnvGuard::set(vec![
			(APP_ID_ENV, Some("12345")),
			(PRIVATE_KEY_PEM_ENV, Some("\\n\\n")),
			(WEBHOOK_SECRET_ENV, Some("webhook-secret")),
			(API_BASE_URL_ENV, Some("https://github.example.test/api/v3")),
		]);

		// Act
		let err = GitHubAppSettings::from_env()
			.expect_err("escaped blank private key should be rejected");

		// Assert
		assert_eq!(
			err.to_string(),
			"REINHARDT_CLOUD_GITHUB_APP_PRIVATE_KEY_PEM is required"
		);
	}

	#[rstest]
	fn test_github_app_settings_debug_redacts_secrets() {
		// Arrange
		let settings = GitHubAppSettings {
			app_id: 12345,
			private_key_pem: "secret-private-key".to_string(),
			webhook_secret: "secret-webhook-token".to_string(),
			api_base_url: "https://api.github.com".to_string(),
		};

		// Act
		let debug = format!("{settings:?}");

		// Assert
		assert!(!debug.contains("secret-private-key"));
		assert!(!debug.contains("secret-webhook-token"));
		assert!(debug.contains("[redacted]"));
		assert!(debug.contains("GitHubAppSettings"));
		assert!(debug.contains("https://api.github.com"));
	}
}

#[cfg(test)]
pub mod model_tests {
	// Included migration files keep `pub fn migration()` because production
	// discovery loads that symbol from standalone migration modules.
	#[allow(unreachable_pub)]
	mod github_initial_migration {
		include!(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/migrations/github/0001_initial.rs"
		));
	}

	use reinhardt::db::migrations::operations::{ColumnDefinition, Operation};
	use reinhardt::db::orm::Model;
	use rstest::rstest;

	use crate::apps::github::models::{GitHubInstallation, GitHubProject, GitHubRepository};

	#[rstest]
	fn test_github_installation_build_sets_fields() {
		// Arrange
		let organization_id = 42i64;
		let installation_id = 123_456i64;
		let account_login = "kent8192".to_string();
		let status = "active".to_string();

		// Act
		let installation = GitHubInstallation::build()
			.organization(organization_id)
			.installation_id(installation_id)
			.account_id(987_654)
			.account_login(account_login.clone())
			.account_type("Organization".to_string())
			.status(status.clone())
			.finish();

		// Assert
		assert_eq!(GitHubInstallation::app_label(), "github");
		assert_eq!(GitHubInstallation::table_name(), "github_installations");
		assert_eq!(installation.id, None);
		assert_eq!(*installation.organization_id(), organization_id);
		assert_eq!(installation.installation_id, installation_id);
		assert_eq!(installation.account_login, account_login);
		assert_eq!(installation.status, status);
	}

	#[rstest]
	fn test_github_repository_build_sets_fields() {
		// Arrange
		let installation_id = 7i64;
		let github_repository_id = 123_456_789i64;
		let full_name = "kent8192/reinhardt-cloud".to_string();
		let default_branch = "main".to_string();

		// Act
		let repository = GitHubRepository::build()
			.installation(installation_id)
			.github_repository_id(github_repository_id)
			.full_name(full_name.clone())
			.owner_login("kent8192".to_string())
			.name("reinhardt-cloud".to_string())
			.default_branch(default_branch.clone())
			.private(true)
			.selected(false)
			.finish();

		// Assert
		assert_eq!(GitHubRepository::app_label(), "github");
		assert_eq!(GitHubRepository::table_name(), "github_repositories");
		assert_eq!(repository.id, None);
		assert_eq!(*repository.installation_id(), installation_id);
		assert_eq!(repository.github_repository_id, github_repository_id);
		assert_eq!(repository.full_name, full_name);
		assert_eq!(repository.default_branch, default_branch);
		assert!(repository.private);
		assert!(!repository.selected);
	}

	#[rstest]
	fn test_github_project_build_sets_fields() {
		// Arrange
		let organization_id = 42i64;
		let repository_id = 7i64;
		let deployment_id = 11i64;

		// Act
		let project = GitHubProject::build()
			.organization(organization_id)
			.repository(repository_id)
			.deployment(deployment_id)
			.app_name("reinhardt-cloud".to_string())
			.production_branch("main".to_string())
			.status("imported".to_string())
			.finish();

		// Assert
		assert_eq!(GitHubProject::app_label(), "github");
		assert_eq!(GitHubProject::table_name(), "github_projects");
		assert_eq!(project.id, None);
		assert_eq!(*project.organization_id(), organization_id);
		assert_eq!(*project.repository_id(), repository_id);
		assert_eq!(*project.deployment_id(), deployment_id);
		assert_eq!(project.app_name, "reinhardt-cloud");
		assert_eq!(project.production_branch, "main");
		assert_eq!(project.status, "imported");
	}

	#[rstest]
	fn test_github_initial_migration_matches_persistent_models() {
		// Arrange
		let migration = github_initial_migration::migration();

		// Act
		let installation_columns =
			create_table_columns(&migration.operations, "github_installations");
		let repository_columns = create_table_columns(&migration.operations, "github_repositories");

		// Assert
		assert_eq!(migration.app_label, GitHubInstallation::app_label());
		assert_eq!(migration.name, "0001_initial");
		assert_eq!(
			migration.dependencies,
			vec![
				("organizations".to_string(), "0001_initial".to_string()),
				(
					"deployments".to_string(),
					"0005_add_reinhardt_app_yaml".to_string()
				)
			]
		);
		assert_column(
			installation_columns,
			"id",
			"BigInteger",
			true,
			false,
			true,
			true,
		);
		assert_column(
			installation_columns,
			"organization_id",
			"BigInteger",
			false,
			false,
			true,
			false,
		);
		assert_column(
			installation_columns,
			"installation_id",
			"BigInteger",
			false,
			true,
			true,
			false,
		);
		assert_column(
			installation_columns,
			"account_login",
			"VarChar(255)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			installation_columns,
			"account_type",
			"VarChar(32)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			installation_columns,
			"status",
			"VarChar(32)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			repository_columns,
			"id",
			"BigInteger",
			true,
			false,
			true,
			true,
		);
		assert_column(
			repository_columns,
			"installation_id",
			"BigInteger",
			false,
			false,
			true,
			false,
		);
		assert_column(
			repository_columns,
			"github_repository_id",
			"BigInteger",
			false,
			true,
			true,
			false,
		);
		assert_column(
			repository_columns,
			"full_name",
			"VarChar(512)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			repository_columns,
			"owner_login",
			"VarChar(255)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			repository_columns,
			"name",
			"VarChar(255)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			repository_columns,
			"default_branch",
			"VarChar(255)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			repository_columns,
			"private",
			"Boolean",
			false,
			false,
			true,
			false,
		);
		assert_column(
			repository_columns,
			"selected",
			"Boolean",
			false,
			false,
			true,
			false,
		);
		let project_columns = create_table_columns(&migration.operations, "github_projects");
		assert_column(project_columns, "id", "BigInteger", true, false, true, true);
		assert_column(
			project_columns,
			"organization_id",
			"BigInteger",
			false,
			false,
			true,
			false,
		);
		assert_column(
			project_columns,
			"repository_id",
			"BigInteger",
			false,
			true,
			true,
			false,
		);
		assert_column(
			project_columns,
			"deployment_id",
			"BigInteger",
			false,
			true,
			true,
			false,
		);
		assert_column(
			project_columns,
			"app_name",
			"VarChar(63)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			project_columns,
			"production_branch",
			"VarChar(255)",
			false,
			false,
			true,
			false,
		);
		assert_column(
			project_columns,
			"status",
			"VarChar(32)",
			false,
			false,
			true,
			false,
		);
		assert!(
			has_constraint(
				&migration.operations,
				"github_installations",
				"github_installations_organization_id_fk"
			),
			"github_installations must reference organizations"
		);
		assert!(
			has_constraint(
				&migration.operations,
				"github_repositories",
				"github_repositories_installation_id_fk"
			),
			"github_repositories must reference github_installations"
		);
		assert!(
			has_constraint(
				&migration.operations,
				"github_projects",
				"github_projects_repository_id_fk"
			),
			"github_projects must reference github_repositories"
		);
		assert!(
			has_constraint(
				&migration.operations,
				"github_projects",
				"github_projects_deployment_id_fk"
			),
			"github_projects must reference deployments"
		);
		assert!(
			has_index(
				&migration.operations,
				"github_installations",
				"organization_id"
			),
			"github_installations.organization_id must be indexed"
		);
		assert!(
			has_index(
				&migration.operations,
				"github_repositories",
				"installation_id"
			),
			"github_repositories.installation_id must be indexed"
		);
		assert!(
			has_index(&migration.operations, "github_projects", "organization_id"),
			"github_projects.organization_id must be indexed"
		);
	}

	fn create_table_columns<'a>(
		operations: &'a [Operation],
		table_name: &str,
	) -> &'a [ColumnDefinition] {
		operations
			.iter()
			.find_map(|operation| match operation {
				Operation::CreateTable { name, columns, .. } if name == table_name => {
					Some(columns.as_slice())
				}
				_ => None,
			})
			.unwrap_or_else(|| panic!("{table_name} table must be created"))
	}

	fn assert_column(
		columns: &[ColumnDefinition],
		name: &str,
		field_type: &str,
		primary_key: bool,
		unique: bool,
		not_null: bool,
		auto_increment: bool,
	) {
		let column = columns
			.iter()
			.find(|column| column.name == name)
			.unwrap_or_else(|| panic!("{name} column must exist"));
		assert_eq!(format!("{:?}", column.type_definition), field_type);
		assert_eq!(column.primary_key, primary_key, "{name}.primary_key");
		assert_eq!(column.unique, unique, "{name}.unique");
		assert_eq!(column.not_null, not_null, "{name}.not_null");
		assert_eq!(
			column.auto_increment, auto_increment,
			"{name}.auto_increment"
		);
	}

	fn has_constraint(operations: &[Operation], table_name: &str, constraint_name: &str) -> bool {
		operations.iter().any(|operation| {
			matches!(
				operation,
				Operation::AddConstraint {
					table,
					constraint_sql,
				} if table == table_name && constraint_sql.contains(constraint_name)
			)
		})
	}

	fn has_index(operations: &[Operation], table_name: &str, column_name: &str) -> bool {
		operations.iter().any(|operation| {
			matches!(
				operation,
				Operation::CreateIndex {
					table,
					columns,
					unique: false,
					..
				} if table == table_name && columns == &vec![column_name.to_string()]
			)
		})
	}
}

#[cfg(test)]
pub mod server_fn_tests {
	use rstest::rstest;

	use crate::apps::github::models::{GitHubInstallation, GitHubProject, GitHubRepository};
	use crate::apps::github::server_fn::{
		github_installation_info, github_project_info, github_repository_info,
		repository_from_installation_repository,
	};
	use crate::apps::github::services::client::GitHubInstallationRepository;

	#[rstest]
	fn test_github_installation_info_maps_display_fields() {
		// Arrange
		let mut installation = GitHubInstallation::build()
			.organization(42)
			.installation_id(123_456)
			.account_id(987_654)
			.account_login("kent8192".to_string())
			.account_type("Organization".to_string())
			.status("active".to_string())
			.finish();
		installation.id = Some(7);

		// Act
		let info = github_installation_info(installation);

		// Assert
		assert_eq!(info.id, 7);
		assert_eq!(info.installation_id, 123_456);
		assert_eq!(info.account_login, "kent8192");
		assert_eq!(info.account_type, "Organization");
		assert_eq!(info.status, "active");
	}

	#[rstest]
	fn test_github_repository_info_maps_repository_fields() {
		// Arrange
		let mut repository = GitHubRepository::build()
			.installation(7)
			.github_repository_id(123_456_789)
			.full_name("kent8192/reinhardt-cloud".to_string())
			.owner_login("kent8192".to_string())
			.name("reinhardt-cloud".to_string())
			.default_branch("main".to_string())
			.private(true)
			.selected(false)
			.finish();
		repository.id = Some(11);

		// Act
		let info = github_repository_info(repository);

		// Assert
		assert_eq!(info.id, 11);
		assert_eq!(info.github_repository_id, 123_456_789);
		assert_eq!(info.full_name, "kent8192/reinhardt-cloud");
		assert_eq!(info.owner_login, "kent8192");
		assert_eq!(info.name, "reinhardt-cloud");
		assert_eq!(info.default_branch, "main");
		assert!(info.private);
		assert!(!info.selected);
	}

	#[rstest]
	fn test_repository_from_installation_repository_maps_cache_row() {
		// Arrange
		let repository = GitHubInstallationRepository {
			owner_login: "kent8192".to_string(),
			id: 123_456_789,
			full_name: "kent8192/reinhardt-cloud".to_string(),
			name: "reinhardt-cloud".to_string(),
			private: true,
			default_branch: "develop/0.2.0".to_string(),
		};

		// Act
		let row = repository_from_installation_repository(7, repository);

		// Assert
		assert_eq!(row.id, None);
		assert_eq!(*row.installation_id(), 7);
		assert_eq!(row.github_repository_id, 123_456_789);
		assert_eq!(row.full_name, "kent8192/reinhardt-cloud");
		assert_eq!(row.owner_login, "kent8192");
		assert_eq!(row.name, "reinhardt-cloud");
		assert_eq!(row.default_branch, "develop/0.2.0");
		assert!(row.private);
		assert!(!row.selected);
	}

	#[rstest]
	fn test_github_project_info_maps_project_fields() {
		// Arrange
		let mut project = GitHubProject::build()
			.organization(42)
			.repository(7)
			.deployment(11)
			.app_name("reinhardt-cloud".to_string())
			.production_branch("main".to_string())
			.status("imported".to_string())
			.finish();
		project.id = Some(13);

		// Act
		let info = github_project_info(project);

		// Assert
		assert_eq!(info.id, 13);
		assert_eq!(info.repository_id, 7);
		assert_eq!(info.deployment_id, 11);
		assert_eq!(info.app_name, "reinhardt-cloud");
		assert_eq!(info.production_branch, "main");
		assert_eq!(info.status, "imported");
	}
}
