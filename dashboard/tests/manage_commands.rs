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
use cloud_control_plane::apps::accounts::services::server::redis_handle::RedisHandle;
use cloud_control_plane::apps::accounts::services::server::sessions::{
	IssuedSession, SessionService,
};
use cloud_control_plane::persisted_time::persisted_now;
use reinhardt::conf::settings::secret_types::SecretString;
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

	/// Sessions in the host's Redis, for tests that plant or inspect them.
	fn sessions(&self) -> SessionService {
		let url = self
			.env
			.iter()
			.find(|(name, _)| *name == "REINHARDT_CLOUD_REDIS_URL")
			.map(|(_, value)| value.clone())
			.expect("the host has a Redis URL");
		SessionService::new(RedisHandle::new(&SecretString::new(url)).expect("a valid Redis URL"))
	}

	/// Stop Redis, as an outage would.
	async fn stop_redis(&self) {
		self._redis.stop().await.expect("Redis stops");
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
	let mut lifetimes: Vec<chrono::Duration> = LoginLink::objects()
		.all()
		.all()
		.await
		.unwrap()
		.iter()
		.map(|row| row.expires_at - row.created_at)
		.collect();
	lifetimes.sort_unstable();
	assert_eq!(lifetimes.len(), 2, "the refused requests stored nothing");
	// `created_at` is stamped by the framework and `expires_at` by the command a
	// moment apart, so compare with a tolerance, as the default-lifetime test does.
	for (lifetime, minutes) in lifetimes.iter().zip([2, 15]) {
		assert!(
			(*lifetime - chrono::Duration::minutes(minutes))
				.num_seconds()
				.abs() < 5,
			"expected about {minutes} minutes, got {lifetime}"
		);
	}
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

async fn set_active(github_user_id: i64, active: bool) {
	User::objects()
		.filter(User::field_github_user_id().eq(github_user_id))
		.update_fields([User::field_is_active().assign(active)])
		.await
		.unwrap();
}

async fn mark_signed_in(github_user_id: i64) {
	User::objects()
		.filter(User::field_github_user_id().eq(github_user_id))
		.update_fields([User::field_last_login().assign(Some(persisted_now()))])
		.await
		.unwrap();
}

/// A signed-in Staff User with a live session, as after a normal sign-in.
async fn staff_with_session(host: &Host, github_user_id: i64) -> (User, IssuedSession) {
	let run = host.manage(&[
		"grant-staff",
		"--github-user-id",
		&github_user_id.to_string(),
	]);
	assert_eq!(run.status, 0, "{}", run.stderr);
	mark_signed_in(github_user_id).await;
	let user = user_of(github_user_id).await.unwrap();
	let session = host.sessions().create(user.id).await.unwrap();
	(user, session)
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_end_sessions_removes_every_session_of_the_user() {
	// Arrange
	let host = Host::start().await;
	let (user, first) = staff_with_session(&host, 70_040).await;
	let second = host.sessions().create(user.id).await.unwrap();

	// Act
	let run = host.manage(&["end-sessions", "--github-user-id", "70040"]);

	// Assert
	assert_eq!(run.status, 0, "{}", run.stderr);
	assert!(run.stdout.contains("Ended 2 session(s)"), "{}", run.stdout);
	for session in [&first, &second] {
		assert_eq!(host.sessions().resolve(&session.token).await.unwrap(), None);
	}
	let records = run.audit_records();
	assert_eq!(records.len(), 1, "{}", run.stderr);
	assert_eq!(records[0]["event"], "accounts.end_sessions.succeeded");
	assert_eq!(records[0]["actor_kind"], "host_operator");
	assert_eq!(records[0]["github_user_id"], 70_040);
	assert!(
		user_of(70_040).await.unwrap().is_staff,
		"ending sessions changes nothing else"
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_end_sessions_exits_non_zero_when_redis_fails() {
	// Arrange
	let host = Host::start().await;
	staff_with_session(&host, 70_041).await;
	host.stop_redis().await;

	// Act
	let run = host.manage(&["end-sessions", "--github-user-id", "70041"]);

	// Assert
	assert_ne!(run.status, 0);
	assert!(run.stdout.is_empty(), "{}", run.stdout);
	assert_eq!(run.audit_events(), ["accounts.end_sessions.failed"]);
	assert_eq!(run.audit_records()[0]["reason"], "sessions_not_ended");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_reactivate_user_ends_sessions_first_so_none_comes_back() {
	// Arrange: deactivated, with a session nobody presented while inactive.
	let host = Host::start().await;
	let (_, leftover) = staff_with_session(&host, 70_042).await;
	set_active(70_042, false).await;

	// Act
	let run = host.manage(&["reactivate-user", "--github-user-id", "70042"]);

	// Assert
	assert_eq!(run.status, 0, "{}", run.stderr);
	assert!(run.stdout.contains("is active again"), "{}", run.stdout);
	assert!(
		run.stdout.contains("1 session(s) ended first"),
		"{}",
		run.stdout
	);
	assert!(user_of(70_042).await.unwrap().is_active);
	assert_eq!(
		host.sessions().resolve(&leftover.token).await.unwrap(),
		None,
		"a pre-existing session is refused once the User is active again"
	);
	let records = run.audit_records();
	assert_eq!(records.len(), 1, "{}", run.stderr);
	assert_eq!(records[0]["event"], "accounts.reactivate.succeeded");
	assert_eq!(records[0]["reason"], "reactivated");
	assert_eq!(records[0]["github_user_id"], 70_042);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_reactivate_user_refuses_when_the_session_step_fails() {
	// Arrange
	let host = Host::start().await;
	staff_with_session(&host, 70_043).await;
	set_active(70_043, false).await;
	host.stop_redis().await;

	// Act
	let run = host.manage(&["reactivate-user", "--github-user-id", "70043"]);

	// Assert
	assert_ne!(run.status, 0);
	assert!(
		!user_of(70_043).await.unwrap().is_active,
		"the User stays inactive when old sessions could not be ended"
	);
	assert_eq!(run.audit_events(), ["accounts.reactivate.refused"]);
	assert_eq!(run.audit_records()[0]["reason"], "sessions_not_ended");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_reactivating_an_active_user_is_unchanged() {
	// Arrange
	let host = Host::start().await;
	let (user, session) = staff_with_session(&host, 70_044).await;

	// Act
	let run = host.manage(&["reactivate-user", "--github-user-id", "70044"]);

	// Assert
	assert_eq!(run.status, 0, "{}", run.stderr);
	assert!(run.stdout.contains("already active"), "{}", run.stdout);
	assert_eq!(run.audit_records()[0]["reason"], "unchanged");
	assert_eq!(
		host.sessions().resolve(&session.token).await.unwrap(),
		Some(user.id),
		"an unchanged User keeps their session"
	);
}

/// The only active Staff User is deactivated by the re-pointing fallback. Nobody
/// can use the admin site (it needs active Staff) and the User cannot sign in, so
/// recovery has to work from the shell alone.
#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_the_only_staff_user_recovers_through_the_commands_alone() {
	// Arrange
	let host = Host::start().await;
	let (user, leftover) = staff_with_session(&host, 70_045).await;
	set_active(70_045, false).await;
	let active_staff = User::objects()
		.filter(User::field_is_staff().eq(true))
		.filter(User::field_is_active().eq(true))
		.all()
		.await
		.unwrap();
	assert!(active_staff.is_empty(), "nobody can reach the admin site");

	// Act
	let ended = host.manage(&["end-sessions", "--github-user-id", "70045"]);
	let reactivated = host.manage(&["reactivate-user", "--github-user-id", "70045"]);

	// Assert
	assert_eq!(
		(ended.status, reactivated.status),
		(0, 0),
		"{}",
		reactivated.stderr
	);
	let recovered = user_of(70_045).await.unwrap();
	assert_eq!(recovered.id, user.id);
	assert!(recovered.is_active && recovered.is_staff);
	assert_eq!(
		host.sessions().resolve(&leftover.token).await.unwrap(),
		None
	);
	assert_eq!(ended.audit_events(), ["accounts.end_sessions.succeeded"]);
	assert_eq!(
		reactivated.audit_events(),
		["accounts.reactivate.succeeded"]
	);
}

#[rstest]
#[case::end_sessions(&["end-sessions"])]
#[case::reactivate_user(&["reactivate-user"])]
#[case::reactivate_zero(&["reactivate-user", "--github-user-id", "0"])]
#[case::deactivate_user(&["deactivate-user"])]
#[tokio::test]
#[serial(database)]
async fn sr_107_the_recovery_commands_require_a_positive_numeric_id(#[case] args: &[&str]) {
	// Arrange
	let host = Host::start().await;

	// Act
	let run = host.manage(args);

	// Assert
	assert_eq!(run.status, 2, "{}", run.stderr);
	assert!(run.audit_events().is_empty());
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_deactivate_user_deactivates_ends_sessions_and_is_undone_by_reactivate_user() {
	// Arrange
	let host = Host::start().await;
	let (user, session) = staff_with_session(&host, 70_060).await;

	// Act
	let deactivated = host.manage(&["deactivate-user", "--github-user-id", "70060"]);
	let again = host.manage(&["deactivate-user", "--github-user-id", "70060"]);

	// Assert
	assert_eq!(deactivated.status, 0, "{}", deactivated.stderr);
	assert!(
		deactivated
			.stdout
			.contains("is deactivated; 1 session(s) ended"),
		"{}",
		deactivated.stdout
	);
	let after = user_of(70_060).await.unwrap();
	assert_eq!(after.id, user.id);
	assert!(!after.is_active && after.is_staff);
	assert_eq!(host.sessions().resolve(&session.token).await.unwrap(), None);
	let records = deactivated.audit_records();
	assert_eq!(records.len(), 1, "{}", deactivated.stderr);
	assert_eq!(records[0]["event"], "accounts.deactivate.succeeded");
	assert_eq!(records[0]["reason"], "deactivated");
	assert_eq!(records[0]["actor_kind"], "host_operator");
	assert_eq!(records[0]["github_user_id"], 70_060);

	assert_eq!(again.status, 0, "{}", again.stderr);
	assert!(
		again.stdout.contains("already inactive"),
		"{}",
		again.stdout
	);
	assert_eq!(again.audit_records()[0]["reason"], "unchanged");

	let back = host.manage(&["reactivate-user", "--github-user-id", "70060"]);
	assert_eq!(back.status, 0, "{}", back.stderr);
	assert!(user_of(70_060).await.unwrap().is_active);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_deactivate_user_succeeds_without_redis_and_says_so() {
	// Arrange: Redis is down, so the sessions cannot be deleted.
	let host = Host::start().await;
	staff_with_session(&host, 70_061).await;
	host.stop_redis().await;

	// Act
	let run = host.manage(&["deactivate-user", "--github-user-id", "70061"]);

	// Assert
	assert_eq!(
		run.status, 0,
		"the inactive flag is what stops the User: {}",
		run.stderr
	);
	assert!(!user_of(70_061).await.unwrap().is_active);
	assert!(run.stdout.contains("is deactivated"), "{}", run.stdout);
	assert!(
		run.stderr
			.contains("manage end-sessions --github-user-id 70061"),
		"{}",
		run.stderr
	);
	assert_eq!(
		run.audit_events(),
		[
			"accounts.deactivate.succeeded",
			"accounts.deactivate.failed"
		]
	);
	assert_eq!(run.audit_records()[1]["reason"], "sessions_not_ended");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_deactivate_user_refuses_an_unknown_user() {
	// Arrange
	let host = Host::start().await;

	// Act
	let run = host.manage(&["deactivate-user", "--github-user-id", "70062"]);

	// Assert
	assert_ne!(run.status, 0);
	assert_eq!(run.audit_events(), ["accounts.deactivate.denied"]);
	assert_eq!(run.audit_records()[0]["reason"], "unknown_user");
}
