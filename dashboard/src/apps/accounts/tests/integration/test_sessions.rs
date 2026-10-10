//! Tests of the Redis-backed browser sessions and sign-in notices (SR-07, SR-08).

use std::time::Duration;

use redis::AsyncCommands;
use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::test::fixtures::{ContainerAsync, GenericImage, redis_container};
use rstest::rstest;
use uuid::Uuid;

use crate::apps::accounts::services::server::redis_handle::RedisHandle;
use crate::apps::accounts::services::server::sessions::{
	SessionPolicy, SessionService, SessionToken,
};
use crate::apps::accounts::services::server::sign_in_notices::{
	NoticeKind, NoticeStore, StoredNotice,
};

struct RedisFixture {
	_container: ContainerAsync<GenericImage>,
	handle: RedisHandle,
	url: String,
}

async fn redis() -> RedisFixture {
	let (container, _port, url) = redis_container().await;
	let handle = RedisHandle::new(&SecretString::new(url.clone())).expect("the URL is valid");
	RedisFixture {
		_container: container,
		handle,
		url,
	}
}

fn short_policy(idle_ms: u64, absolute_ms: u64) -> SessionPolicy {
	SessionPolicy {
		idle: Duration::from_millis(idle_ms),
		absolute: Duration::from_millis(absolute_ms),
	}
}

#[rstest]
fn sr_08_the_standard_limits_are_thirty_minutes_idle_and_twenty_four_hours_total() {
	// Arrange / Act
	let policy = SessionPolicy::STANDARD;

	// Assert
	assert_eq!(policy.idle, Duration::from_secs(30 * 60));
	assert_eq!(policy.absolute, Duration::from_secs(24 * 60 * 60));
}

#[rstest]
#[tokio::test]
async fn sr_08_every_session_gets_a_new_unguessable_token() {
	// Arrange
	let redis = redis().await;
	let sessions = SessionService::new(redis.handle.clone());
	let user = Uuid::now_v7();

	// Act
	let first = sessions.create(user).await.unwrap();
	let second = sessions.create(user).await.unwrap();

	// Assert
	assert_ne!(first.token, second.token);
	assert_eq!(first.token.expose().len(), 43, "256 bits, base64url");
	assert_eq!(first.lifetime, Duration::from_secs(24 * 60 * 60));
	assert_eq!(sessions.resolve(&first.token).await.unwrap(), Some(user));
	assert_eq!(sessions.resolve(&second.token).await.unwrap(), Some(user));
}

#[rstest]
#[tokio::test]
async fn sr_08_redis_never_holds_a_usable_cookie_value() {
	// Arrange
	let redis = redis().await;
	let sessions = SessionService::new(redis.handle.clone());
	let issued = sessions.create(Uuid::now_v7()).await.unwrap();
	let client = redis::Client::open(redis.url.as_str()).unwrap();
	let mut connection = client.get_multiplexed_async_connection().await.unwrap();

	// Act
	let keys: Vec<String> = connection.keys("*").await.unwrap();

	// Assert
	assert!(!keys.is_empty());
	assert!(
		keys.iter().all(|key| !key.contains(issued.token.expose())),
		"a key must hold the token's digest, never the token: {keys:?}"
	);
}

#[rstest]
#[tokio::test]
async fn sr_08_an_unknown_or_malformed_token_resolves_to_nobody() {
	// Arrange
	let redis = redis().await;
	let sessions = SessionService::new(redis.handle.clone());

	// Act
	let unknown = sessions
		.resolve(&SessionToken::from_cookie("not-a-session"))
		.await
		.unwrap();
	let empty = sessions
		.resolve(&SessionToken::from_cookie(""))
		.await
		.unwrap();

	// Assert
	assert_eq!(unknown, None);
	assert_eq!(empty, None);
}

#[rstest]
#[tokio::test]
async fn sr_08_destroy_removes_the_session_on_the_server() {
	// Arrange
	let redis = redis().await;
	let sessions = SessionService::new(redis.handle.clone());
	let user = Uuid::now_v7();
	let issued = sessions.create(user).await.unwrap();
	let copied = SessionToken::from_cookie(issued.token.expose());

	// Act
	let destroyed = sessions.destroy(&issued.token).await.unwrap();
	let again = sessions.destroy(&issued.token).await.unwrap();

	// Assert
	assert_eq!(destroyed, Some(user));
	assert_eq!(again, None, "destroying twice is harmless");
	assert_eq!(
		sessions.resolve(&copied).await.unwrap(),
		None,
		"a copied cookie stops working with the session"
	);
}

#[rstest]
#[tokio::test]
async fn sr_08_an_idle_session_expires() {
	// Arrange
	let redis = redis().await;
	let sessions = SessionService::with_policy(redis.handle.clone(), short_policy(400, 60_000));
	let issued = sessions.create(Uuid::now_v7()).await.unwrap();

	// Act
	tokio::time::sleep(Duration::from_millis(900)).await;
	let resolved = sessions.resolve(&issued.token).await.unwrap();

	// Assert
	assert_eq!(resolved, None);
}

#[rstest]
#[tokio::test]
async fn sr_08_using_a_session_slides_the_idle_timer() {
	// Arrange
	let redis = redis().await;
	let sessions = SessionService::with_policy(redis.handle.clone(), short_policy(1_200, 60_000));
	let user = Uuid::now_v7();
	let issued = sessions.create(user).await.unwrap();

	// Act: stay active for longer than the idle limit, in steps shorter than it.
	for _ in 0..4 {
		tokio::time::sleep(Duration::from_millis(500)).await;
		assert_eq!(sessions.resolve(&issued.token).await.unwrap(), Some(user));
	}

	// Assert
	assert_eq!(sessions.resolve(&issued.token).await.unwrap(), Some(user));
}

#[rstest]
#[tokio::test]
async fn sr_08_a_session_ends_at_the_absolute_limit_however_active_it_is() {
	// Arrange
	let redis = redis().await;
	let sessions = SessionService::with_policy(redis.handle.clone(), short_policy(1_000, 1_600));
	let user = Uuid::now_v7();
	let issued = sessions.create(user).await.unwrap();

	// Act: keep using it so that the idle timer never runs out.
	let mut last_alive = None;
	for step in 0..6 {
		tokio::time::sleep(Duration::from_millis(400)).await;
		let resolved = sessions.resolve(&issued.token).await.unwrap();
		if resolved.is_some() {
			last_alive = Some(step);
		}
	}

	// Assert
	assert!(last_alive.is_some(), "the session was alive at first");
	assert_eq!(
		sessions.resolve(&issued.token).await.unwrap(),
		None,
		"the absolute limit ends an active session"
	);
}

#[rstest]
#[tokio::test]
async fn sr_08_every_session_of_a_user_can_be_destroyed_at_once() {
	// Arrange
	let redis = redis().await;
	let sessions = SessionService::new(redis.handle.clone());
	let user = Uuid::now_v7();
	let other = Uuid::now_v7();
	let first = sessions.create(user).await.unwrap();
	let second = sessions.create(user).await.unwrap();
	let unrelated = sessions.create(other).await.unwrap();

	// Act
	let destroyed = sessions.destroy_all_for_user(user).await.unwrap();

	// Assert
	assert_eq!(destroyed, 2);
	assert_eq!(sessions.resolve(&first.token).await.unwrap(), None);
	assert_eq!(sessions.resolve(&second.token).await.unwrap(), None);
	assert_eq!(
		sessions.resolve(&unrelated.token).await.unwrap(),
		Some(other),
		"another User's session is untouched"
	);
}

#[rstest]
#[tokio::test]
async fn sign_in_notices_are_redeemed_exactly_once() {
	// Arrange
	let redis = redis().await;
	let notices = NoticeStore::new(redis.handle.clone());
	let notice = StoredNotice {
		kind: NoticeKind::NotInvited,
		login: Some("riley-chen".to_owned()),
	};
	let id = notices.put(&notice).await.unwrap();

	// Act
	let first = notices.take(&id).await.unwrap();
	let second = notices.take(&id).await.unwrap();
	let unknown = notices.take("never-issued").await.unwrap();

	// Assert
	assert_eq!(first, Some(notice));
	assert_eq!(second, None);
	assert_eq!(unknown, None);
}
