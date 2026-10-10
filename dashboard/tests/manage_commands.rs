//! The accounts `manage` commands, run as the real `manage` binary against a
//! real PostgreSQL and Redis: `grant-staff`, `create-login-link`, and
//! `repoint-github-account` (SR-16 to SR-18, SR-20, SR-105, SR-107).
//!
//! What a service-level test cannot show is the contract of the command line:
//! argument parsing, the validated-settings gate, what reaches standard output
//! (only the Login Link URL, once) and standard error (the audit records), and
//! that the secret appears nowhere but that one line.

use std::fmt::Write as _;
use std::process::{Command, Output};

use cloud_control_plane::apps::accounts::models::{LoginLink, User};
use cloud_control_plane::persisted_time::persisted_now;
use reinhardt::db::orm::Model;
use reinhardt::test::fixtures::{
	ContainerAsync, GenericImage, MigrationDatabase, postgres_with_migrations_from_dir,
	redis_container,
};
use rstest::rstest;
use serde_json::Value;
use serial_test::serial;
use sha2::{Digest, Sha256};

const PUBLIC_URL: &str = "https://cloud.example.test";

/// A database, a Redis, and the environment a deployment would give `manage`.
struct Host {
	_postgres: ContainerAsync<GenericImage>,
	_connection: MigrationDatabase,
	_redis: ContainerAsync<GenericImage>,
	env: Vec<(&'static str, String)>,
}

impl Host {
	async fn start() -> Self {
		let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (postgres, connection) = postgres_with_migrations_from_dir(&migrations)
			.await
			.expect("PostgreSQL with migrations should start");
		let port = postgres
			.get_host_port_ipv4(5432)
			.await
			.expect("PostgreSQL publishes its port");
		let (redis, _port, redis_url) = redis_container().await;
		let env = vec![
			("REINHARDT_ENV", "ci".to_owned()),
			(
				"REINHARDT_CORE__SECRET_KEY",
				"test-only-secret-key-not-for-deployment".to_owned(),
			),
			(
				"REINHARDT_DATABASE_PASSWORD",
				"test-only-database-password".to_owned(),
			),
			("REINHARDT_DATABASE_HOST", "localhost".to_owned()),
			("REINHARDT_DATABASE_PORT", port.to_string()),
			("REINHARDT_DATABASE_NAME", "postgres".to_owned()),
			("REINHARDT_DATABASE_USER", "postgres".to_owned()),
			("REINHARDT_CLOUD_REDIS_URL", redis_url),
			("REINHARDT_CLOUD_PUBLIC_URL", PUBLIC_URL.to_owned()),
		];
		Self {
			_postgres: postgres,
			_connection: connection,
			_redis: redis,
			env,
		}
	}

	fn set(&mut self, name: &'static str, value: &str) {
		self.env.retain(|(existing, _)| *existing != name);
		self.env.push((name, value.to_owned()));
	}

	fn remove(&mut self, name: &'static str) {
		self.env.retain(|(existing, _)| *existing != name);
	}

	/// Run `manage <args>` from the application directory, with exactly this
	/// host's environment plus what the toolchain needs to find the binary.
	fn manage(&self, args: &[&str]) -> Run {
		let mut command = Command::new(env!("CARGO_BIN_EXE_manage"));
		command
			.args(args)
			.current_dir(env!("CARGO_MANIFEST_DIR"))
			.env_clear()
			.env("PATH", std::env::var_os("PATH").unwrap_or_default());
		for (name, value) in &self.env {
			command.env(name, value);
		}
		let output = command.output().expect("the manage binary should run");
		Run::from(output)
	}
}

struct Run {
	status: i32,
	stdout: String,
	stderr: String,
}

impl From<Output> for Run {
	fn from(output: Output) -> Self {
		Self {
			status: output.status.code().unwrap_or(-1),
			stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
			stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		}
	}
}

impl Run {
	/// The `audit` records on standard error, parsed.
	fn audit_records(&self) -> Vec<Value> {
		self.stderr
			.lines()
			.filter_map(|line| serde_json::from_str::<Value>(line).ok())
			.filter(|record| record["target"] == "audit")
			.map(|record| record["fields"].clone())
			.collect()
	}

	fn audit_events(&self) -> Vec<String> {
		self.audit_records()
			.iter()
			.filter_map(|fields| fields["event"].as_str().map(str::to_owned))
			.collect()
	}
}

fn sha256_hex(text: &str) -> String {
	Sha256::digest(text.as_bytes())
		.iter()
		.fold(String::new(), |mut out, byte| {
			let _ = write!(out, "{byte:02x}");
			out
		})
}

async fn user_of(github_user_id: i64) -> Option<User> {
	User::objects()
		.filter(User::field_github_user_id().eq(github_user_id))
		.first()
		.await
		.expect("users should be listed")
}

async fn link_count() -> usize {
	LoginLink::objects()
		.all()
		.all()
		.await
		.expect("links should be listed")
		.len()
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_105_grant_staff_pre_provisions_a_user_who_has_never_signed_in() {
	// Arrange
	let host = Host::start().await;

	// Act
	let run = host.manage(&["grant-staff", "--github-user-id", "70001"]);

	// Assert
	assert_eq!(run.status, 0, "{}", run.stderr);
	assert!(
		run.stdout.contains("pre-provisioned as Staff"),
		"{}",
		run.stdout
	);
	let user = user_of(70_001).await.expect("the User was created");
	assert!(user.is_staff && user.is_active);
	assert_eq!(user.last_login, None);
	let records = run.audit_records();
	assert_eq!(records.len(), 1, "{}", run.stderr);
	assert_eq!(records[0]["event"], "accounts.grant_staff.succeeded");
	assert_eq!(records[0]["actor_kind"], "host_operator");
	assert_eq!(records[0]["github_user_id"], 70_001);
	assert_eq!(records[0]["reason"], "pre_provisioned");
	assert_eq!(records[0]["subject_user_id"], user.id.to_string());
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_20_grant_staff_revoke_removes_staff_from_a_user_who_signed_in() {
	// Arrange
	let host = Host::start().await;
	assert_eq!(
		host.manage(&["grant-staff", "--github-user-id", "70002"])
			.status,
		0
	);
	User::objects()
		.filter(User::field_github_user_id().eq(70_002_i64))
		.update_fields([User::field_last_login().assign(Some(persisted_now()))])
		.await
		.unwrap();

	// Act
	let run = host.manage(&["grant-staff", "--github-user-id", "70002", "--revoke"]);

	// Assert
	assert_eq!(run.status, 0, "{}", run.stderr);
	assert!(run.stdout.contains("no longer Staff"), "{}", run.stdout);
	let user = user_of(70_002).await.expect("a User who signed in stays");
	assert!(!user.is_staff && user.is_active);
	let records = run.audit_records();
	assert_eq!(records[0]["event"], "accounts.revoke_staff.succeeded");
	assert_eq!(records[0]["reason"], "revoked");
	assert_eq!(records[0]["github_user_id"], 70_002);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_105_revoking_a_grant_that_nobody_used_removes_the_user() {
	// Arrange
	let host = Host::start().await;
	host.manage(&["grant-staff", "--github-user-id", "70003"]);

	// Act
	let run = host.manage(&["grant-staff", "--revoke", "--github-user-id", "70003"]);

	// Assert
	assert_eq!(run.status, 0, "{}", run.stderr);
	assert!(user_of(70_003).await.is_none());
	assert_eq!(run.audit_records()[0]["reason"], "pre_provisioned_removed");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_20_revoking_staff_of_an_unknown_id_fails_and_is_audited() {
	// Arrange
	let host = Host::start().await;

	// Act
	let run = host.manage(&["grant-staff", "--github-user-id", "70004", "--revoke"]);

	// Assert
	assert_ne!(run.status, 0);
	assert_eq!(run.audit_events(), ["accounts.revoke_staff.denied"]);
}

#[rstest]
#[case::missing_id(&["grant-staff"])]
#[case::zero(&["grant-staff", "--github-user-id", "0"])]
#[case::negative(&["grant-staff", "--github-user-id", "-4"])]
#[case::a_login_instead_of_the_number(&["grant-staff", "--github-user-id", "octocat"])]
#[tokio::test]
#[serial(database)]
async fn sr_20_grant_staff_takes_only_a_positive_numeric_id(#[case] args: &[&str]) {
	// Arrange
	let host = Host::start().await;

	// Act
	let run = host.manage(args);

	// Assert
	assert_eq!(run.status, 2, "{}", run.stderr);
	assert!(run.stdout.is_empty());
	assert!(run.audit_events().is_empty(), "nothing was attempted");
	assert_eq!(
		User::objects().all().all().await.unwrap().len(),
		0,
		"no User was created"
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_create_login_link_prints_the_url_once_and_stores_only_a_digest() {
	// Arrange
	let host = Host::start().await;
	host.manage(&["grant-staff", "--github-user-id", "70010"]);

	// Act
	let run = host.manage(&["create-login-link", "--github-user-id", "70010"]);

	// Assert
	assert_eq!(run.status, 0, "{}", run.stderr);
	let lines: Vec<&str> = run.stdout.lines().collect();
	assert_eq!(
		lines.len(),
		1,
		"standard output is the URL alone: {:?}",
		run.stdout
	);
	let prefix = format!("{PUBLIC_URL}/sign-in/link/#");
	let secret = lines[0].strip_prefix(&prefix).unwrap_or_else(|| {
		panic!(
			"the URL must carry the secret in its fragment: {}",
			lines[0]
		)
	});
	assert_eq!(secret.len(), 43, "256 bits as base64url");
	assert!(
		secret
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
	);

	assert!(
		!run.stderr.contains(secret),
		"the secret is not logged anywhere"
	);
	let rows = LoginLink::objects().all().all().await.unwrap();
	assert_eq!(rows.len(), 1);
	assert_eq!(rows[0].token_hash, sha256_hex(secret));
	assert_eq!(rows[0].consumed_at, None);
	let user = user_of(70_010).await.unwrap();
	assert_eq!(rows[0].user_id(), user.id);
	let lifetime = rows[0].expires_at - rows[0].created_at;
	assert!(
		(lifetime - chrono::Duration::minutes(10))
			.num_seconds()
			.abs() < 5,
		"the default lifetime is ten minutes: {lifetime}"
	);
	let records = run.audit_records();
	assert_eq!(records.len(), 1, "{}", run.stderr);
	assert_eq!(records[0]["event"], "accounts.login_link.issued");
	assert_eq!(records[0]["actor_kind"], "host_operator");
	assert_eq!(records[0]["github_user_id"], 70_010);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_17_the_lifetime_can_be_shortened_but_never_raised_past_the_ceiling() {
	// Arrange
	let host = Host::start().await;
	host.manage(&["grant-staff", "--github-user-id", "70011"]);

	// Act
	let shorter = host.manage(&[
		"create-login-link",
		"--github-user-id",
		"70011",
		"--ttl-minutes",
		"2",
	]);
	let ceiling = host.manage(&[
		"create-login-link",
		"--github-user-id",
		"70011",
		"--ttl-minutes",
		"15",
	]);
	let above = host.manage(&[
		"create-login-link",
		"--github-user-id",
		"70011",
		"--ttl-minutes",
		"16",
	]);
	let zero = host.manage(&[
		"create-login-link",
		"--github-user-id",
		"70011",
		"--ttl-minutes",
		"0",
	]);

	// Assert
	assert_eq!((shorter.status, ceiling.status), (0, 0));
	assert_eq!((above.status, zero.status), (2, 2), "{}", above.stderr);
	assert!(above.stdout.is_empty() && zero.stdout.is_empty());
	let mut minutes: Vec<i64> = LoginLink::objects()
		.all()
		.all()
		.await
		.unwrap()
		.iter()
		.map(|row| (row.expires_at - row.created_at).num_minutes())
		.collect();
	minutes.sort_unstable();
	assert_eq!(
		minutes,
		[1, 14],
		"2 and 15 minutes, truncated to whole minutes"
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_create_login_link_cannot_create_a_user() {
	// Arrange
	let host = Host::start().await;

	// Act
	let run = host.manage(&["create-login-link", "--github-user-id", "70012"]);

	// Assert
	assert_ne!(run.status, 0);
	assert!(
		run.stdout.is_empty(),
		"no URL for a User that does not exist"
	);
	assert_eq!(link_count().await, 0);
	assert!(user_of(70_012).await.is_none());
	assert_eq!(run.audit_events(), ["accounts.login_link.issue_denied"]);
	assert_eq!(run.audit_records()[0]["reason"], "unknown_user");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_create_login_link_refuses_a_deactivated_user() {
	// Arrange
	let host = Host::start().await;
	host.manage(&["grant-staff", "--github-user-id", "70013"]);
	User::objects()
		.filter(User::field_github_user_id().eq(70_013_i64))
		.update_fields([User::field_is_active().assign(false)])
		.await
		.unwrap();

	// Act
	let run = host.manage(&["create-login-link", "--github-user-id", "70013"]);

	// Assert
	assert_ne!(run.status, 0);
	assert!(run.stdout.is_empty());
	assert_eq!(link_count().await, 0);
	assert_eq!(run.audit_records()[0]["reason"], "user_inactive");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_create_login_link_needs_a_public_origin_to_build_the_url() {
	// Arrange: a deployment with GitHub sign-in disabled may carry no usable
	// public origin; a value that is not an origin must not become a relative or
	// malformed URL.
	let mut host = Host::start().await;
	host.manage(&["grant-staff", "--github-user-id", "70014"]);
	host.set("REINHARDT_CLOUD_PUBLIC_URL", "cloud.example.test/dashboard");

	// Act
	let run = host.manage(&["create-login-link", "--github-user-id", "70014"]);

	// Assert
	assert_ne!(run.status, 0);
	assert!(run.stdout.is_empty(), "never a relative URL");
	assert!(
		run.stderr.contains("REINHARDT_CLOUD_PUBLIC_URL"),
		"{}",
		run.stderr
	);
	assert_eq!(
		link_count().await,
		0,
		"nothing is stored when no URL can be built"
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_99_a_command_refuses_to_run_with_unusable_settings() {
	// Arrange
	let mut host = Host::start().await;
	host.remove("REINHARDT_CORE__SECRET_KEY");

	// Act
	let run = host.manage(&["grant-staff", "--github-user-id", "70020"]);

	// Assert
	assert_ne!(run.status, 0);
	assert!(
		run.stderr.contains("REINHARDT_CORE__SECRET_KEY"),
		"{}",
		run.stderr
	);
	assert!(user_of(70_020).await.is_none());
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_repoint_moves_the_user_and_refuses_an_id_another_user_has() {
	// Arrange
	let host = Host::start().await;
	host.manage(&["grant-staff", "--github-user-id", "70030"]);
	host.manage(&["grant-staff", "--github-user-id", "70031"]);
	let mover = user_of(70_030).await.unwrap();

	// Act
	let refused = host.manage(&[
		"repoint-github-account",
		"--github-user-id",
		"70030",
		"--new-github-user-id",
		"70031",
	]);
	let moved = host.manage(&[
		"repoint-github-account",
		"--github-user-id",
		"70030",
		"--new-github-user-id",
		"70032",
	]);

	// Assert
	assert_ne!(refused.status, 0);
	assert_eq!(refused.audit_events(), ["accounts.repoint.denied"]);
	assert_eq!(refused.audit_records()[0]["reason"], "target_in_use");
	assert_eq!(moved.status, 0, "{}", moved.stderr);
	assert!(user_of(70_030).await.is_none());
	assert_eq!(user_of(70_032).await.unwrap().id, mover.id);
	assert_eq!(
		moved.audit_events(),
		["accounts.repoint.released", "accounts.repoint.claimed"]
	);
	let ids: Vec<_> = moved
		.audit_records()
		.iter()
		.map(|fields| fields["github_user_id"].clone())
		.collect();
	assert_eq!(ids, [70_030, 70_032]);
}

#[rstest]
#[case::missing_new_id(&["repoint-github-account", "--github-user-id", "5"])]
#[case::missing_current_id(&["repoint-github-account", "--new-github-user-id", "5"])]
#[case::non_numeric(&["repoint-github-account", "--github-user-id", "a", "--new-github-user-id", "5"])]
#[tokio::test]
#[serial(database)]
async fn sr_107_repoint_requires_both_numeric_ids(#[case] args: &[&str]) {
	// Arrange
	let host = Host::start().await;

	// Act
	let run = host.manage(args);

	// Assert
	assert_eq!(run.status, 2, "{}", run.stderr);
	assert!(run.audit_events().is_empty());
}
