//! Shared fixtures and helpers of the accounts tests.

use std::sync::Arc;

use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::core::exception::{DatabaseErrorKind, Error};
use reinhardt::db::orm::Model;
use reinhardt::test::fixtures::{
	ContainerAsync, GenericImage, MigrationDatabase, postgres_with_migrations_from_dir,
	redis_container,
};
use rstest::fixture;

use crate::apps::accounts::models::User;
use crate::apps::accounts::services::server::provider_tokens::OrmSocialAccountStorage;
use crate::apps::accounts::services::server::redis_handle::RedisHandle;
use crate::apps::accounts::services::server::sessions::SessionService;
use crate::apps::accounts::services::server::sign_up_policy::{
	MembershipError, OrganizationMembership,
};
use crate::apps::accounts::services::server::token_crypto::{TokenKeyring, TokenKeyringSettings};
use crate::apps::accounts::services::server::users::GithubProfile;

/// A PostgreSQL container with the committed migrations applied and the ORM
/// pointed at it. Dropping it stops the container.
pub(crate) struct TestDatabase {
	_container: ContainerAsync<GenericImage>,
	_connection: MigrationDatabase,
}

impl TestDatabase {
	/// The host port PostgreSQL listens on. The container trusts every local
	/// connection, as the framework's fixture configures it.
	pub(crate) async fn port(&self) -> u16 {
		self._container
			.get_host_port_ipv4(5432)
			.await
			.expect("PostgreSQL publishes its port")
	}
}

#[fixture]
pub(crate) async fn database() -> TestDatabase {
	let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let (container, connection) = postgres_with_migrations_from_dir(&migrations)
		.await
		.expect("PostgreSQL with migrations should start");
	TestDatabase {
		_container: container,
		_connection: connection,
	}
}

/// Base64 of 32 bytes (all 0x07).
pub(crate) const TEST_KEY: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";
/// Base64 of 32 bytes (all 0x09).
pub(crate) const OTHER_TEST_KEY: &str = "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk=";

pub(crate) fn keyring(key: &str) -> Arc<TokenKeyring> {
	let key = SecretString::new(key);
	Arc::new(
		TokenKeyring::from_settings(
			TokenKeyringSettings {
				key: Some(&key),
				key_id: "",
				retired_keys: None,
			},
			"unused-because-a-dedicated-key-is-set",
		)
		.expect("test key should be valid"),
	)
}

pub(crate) fn storage() -> OrmSocialAccountStorage {
	OrmSocialAccountStorage::new(keyring(TEST_KEY))
}

pub(crate) fn profile(github_user_id: i64, login: &str) -> GithubProfile {
	GithubProfile {
		github_user_id,
		login: login.to_owned(),
		name: None,
		avatar_url: None,
		verified_email: None,
	}
}

/// Insert a User directly, as `manage grant-staff` would.
pub(crate) async fn insert_user(github_user_id: i64, login: &str, is_staff: bool) -> User {
	let user = User::build()
		.github_user_id(github_user_id)
		.github_login(login.to_owned())
		.display_name(login.to_owned())
		.avatar_url(None)
		.email(None)
		.is_active(true)
		.is_staff(is_staff)
		.last_login(None)
		.finish();
	User::objects()
		.create(&user)
		.await
		.expect("user should be created")
}

pub(crate) async fn user_count() -> usize {
	User::objects()
		.all()
		.all()
		.await
		.expect("users should be listed")
		.len()
}

/// Membership lookup that answers from a fixed list.
pub(crate) struct FixedMembership(pub(crate) Result<Vec<i64>, MembershipError>);

#[async_trait::async_trait]
impl OrganizationMembership for FixedMembership {
	async fn organization_ids(&self, _github_user_id: i64) -> Result<Vec<i64>, MembershipError> {
		self.0.clone()
	}
}

/// The kind and constraint name of the database error inside `error`.
pub(crate) fn database_violation(error: &Error) -> Option<(DatabaseErrorKind, Option<String>)> {
	error
		.database_error()
		.map(|database| (database.kind(), database.constraint().map(str::to_owned)))
}

/// A Redis container and the session service on top of it. Dropping it stops
/// the container.
pub(crate) struct TestSessions {
	_container: ContainerAsync<GenericImage>,
	pub(crate) sessions: SessionService,
}

pub(crate) async fn redis_sessions() -> TestSessions {
	let (container, _port, url) = redis_container().await;
	let handle = RedisHandle::new(&SecretString::new(url)).expect("the Redis URL is valid");
	TestSessions {
		_container: container,
		sessions: SessionService::new(handle),
	}
}
