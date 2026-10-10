//! Tests of using a User's GitHub token and renewing it on use (reinhardt-web#6709).

use std::sync::Arc;

use chrono::{Duration, Utc};
use reinhardt::conf::settings::secret_types::SecretString;
use rstest::rstest;
use serde_json::json;
use serial_test::serial;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::apps::accounts::server::settings::GithubAppConfig;
use crate::apps::accounts::services::server::provider_token_refresh::{
	ProviderTokenRefresher, ProviderTokenService, TokenAccessError,
};
use crate::apps::accounts::services::server::provider_tokens::{
	OrmSocialAccountStorage, ProviderTokens,
};
use crate::apps::accounts::tests::support::{TestDatabase, database, insert_user, keyring};

struct Fixture {
	service: ProviderTokenService,
	storage: Arc<OrmSocialAccountStorage>,
	mock: MockServer,
	user: uuid::Uuid,
	_db: TestDatabase,
}

async fn fixture(db: TestDatabase) -> Fixture {
	let mock = MockServer::start().await;
	let config = GithubAppConfig {
		client_id: "Iv1.test-client-id".to_owned(),
		client_secret: SecretString::new("test-client-secret"),
		redirect_uri: "http://127.0.0.1:1/callback/".to_owned(),
		authorize_url: format!("{}/login/oauth/authorize", mock.uri()),
		token_url: format!("{}/login/oauth/access_token", mock.uri()),
		api_url: mock.uri(),
	};
	let storage = Arc::new(OrmSocialAccountStorage::new(keyring(
		crate::apps::accounts::tests::support::TEST_KEY,
	)));
	let service = ProviderTokenService::new(storage.clone(), ProviderTokenRefresher::new(&config));
	let user = insert_user(42, "octocat", false).await.id;
	Fixture {
		service,
		storage,
		mock,
		user,
		_db: db,
	}
}

impl Fixture {
	async fn store(&self, expires_in: Duration, refresh: Option<(&str, Duration)>) {
		let now = Utc::now();
		self.storage
			.store_tokens(
				self.user,
				&ProviderTokens {
					access_token: SecretString::new("old-access"),
					refresh_token: refresh.map(|(token, _)| SecretString::new(token)),
					access_token_expires_at: now + expires_in,
					refresh_token_expires_at: refresh.map(|(_, lifetime)| now + lifetime),
				},
			)
			.await
			.unwrap();
	}

	/// Answer the refresh request the way GitHub does: JSON only when asked for.
	async fn github_refreshes_with(&self, body: serde_json::Value) {
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.and(header("accept", "application/json"))
			.and(body_string_contains("grant_type=refresh_token"))
			.and(body_string_contains("refresh_token=old-refresh"))
			.and(body_string_contains("client_id=Iv1.test-client-id"))
			.and(body_string_contains("client_secret=test-client-secret"))
			.respond_with(ResponseTemplate::new(200).set_body_json(body))
			.mount(&self.mock)
			.await;
		// Without `Accept: application/json` GitHub answers form-encoded, which
		// upstream's `RefreshFlow` cannot parse (reinhardt-web#6709).
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.respond_with(
				ResponseTemplate::new(200)
					.insert_header("content-type", "application/x-www-form-urlencoded")
					.set_body_string(
						"access_token=form-encoded&token_type=bearer&expires_in=28800",
					),
			)
			.mount(&self.mock)
			.await;
	}

	async fn refresh_calls(&self) -> usize {
		self.mock
			.received_requests()
			.await
			.unwrap()
			.iter()
			.filter(|request| request.url.path() == "/login/oauth/access_token")
			.count()
	}
}

fn new_pair() -> serde_json::Value {
	json!({
		"access_token": "new-access",
		"token_type": "bearer",
		"expires_in": 28800,
		"refresh_token": "new-refresh",
		"refresh_token_expires_in": 15811200
	})
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn a_token_that_is_not_about_to_expire_is_used_as_it_is() {
	// Arrange
	let fx = fixture(database().await).await;
	fx.store(
		Duration::hours(2),
		Some(("old-refresh", Duration::days(30))),
	)
	.await;

	// Act
	let token = fx.service.access_token(fx.user).await.unwrap();

	// Assert
	assert_eq!(token.expose_secret(), "old-access");
	assert_eq!(fx.refresh_calls().await, 0);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn an_expiring_token_is_renewed_and_the_rotated_pair_is_stored_together() {
	// Arrange
	let fx = fixture(database().await).await;
	fx.store(
		Duration::seconds(10),
		Some(("old-refresh", Duration::days(30))),
	)
	.await;
	fx.github_refreshes_with(new_pair()).await;

	// Act
	let token = fx.service.access_token(fx.user).await.unwrap();

	// Assert
	assert_eq!(token.expose_secret(), "new-access");
	let stored = fx.storage.load_tokens(fx.user).await.unwrap().unwrap();
	assert_eq!(stored.access_token.expose_secret(), "new-access");
	assert_eq!(
		stored
			.refresh_token
			.as_ref()
			.map(SecretString::expose_secret),
		Some("new-refresh")
	);
	let access_lifetime = (stored.access_token_expires_at - Utc::now()).num_seconds();
	assert!(
		(28_700..=28_800).contains(&access_lifetime),
		"{access_lifetime}"
	);
	let refresh_lifetime = (stored.refresh_token_expires_at.unwrap() - Utc::now()).num_seconds();
	assert!(
		(15_811_100..=15_811_200).contains(&refresh_lifetime),
		"the refresh expiry GitHub reports is kept: {refresh_lifetime}"
	);
	assert_eq!(fx.refresh_calls().await, 1);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn the_refresh_request_asks_github_for_json() {
	// Arrange
	let fx = fixture(database().await).await;
	fx.store(
		Duration::seconds(-5),
		Some(("old-refresh", Duration::days(30))),
	)
	.await;
	fx.github_refreshes_with(new_pair()).await;

	// Act
	let result = fx.service.access_token(fx.user).await;

	// Assert
	assert_eq!(
		result.map(|token| token.expose_secret().to_owned()),
		Ok("new-access".to_owned()),
		"the form-encoded fallback answer would have been unusable"
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn a_rejected_refresh_token_means_the_user_must_sign_in_again() {
	// Arrange
	let fx = fixture(database().await).await;
	fx.store(
		Duration::seconds(10),
		Some(("old-refresh", Duration::days(30))),
	)
	.await;
	fx.github_refreshes_with(json!({
		"error": "bad_refresh_token",
		"error_description": "The refresh token passed is incorrect or expired."
	}))
	.await;

	// Act
	let result = fx.service.access_token(fx.user).await;

	// Assert
	assert_eq!(
		result.err(),
		Some(TokenAccessError::ReauthenticationRequired)
	);
	let stored = fx.storage.load_tokens(fx.user).await.unwrap().unwrap();
	assert_eq!(
		stored.access_token.expose_secret(),
		"old-access",
		"a failed refresh leaves the stored tokens alone"
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn racing_requests_share_one_single_use_refresh_token() {
	// Arrange
	let fx = fixture(database().await).await;
	fx.store(
		Duration::seconds(10),
		Some(("old-refresh", Duration::days(30))),
	)
	.await;
	Mock::given(method("POST"))
		.and(path("/login/oauth/access_token"))
		.and(body_string_contains("refresh_token=old-refresh"))
		.respond_with(ResponseTemplate::new(200).set_body_json(new_pair()))
		.up_to_n_times(1)
		.mount(&fx.mock)
		.await;
	Mock::given(method("POST"))
		.and(path("/login/oauth/access_token"))
		.respond_with(
			ResponseTemplate::new(200).set_body_json(json!({"error": "bad_refresh_token"})),
		)
		.mount(&fx.mock)
		.await;

	// Act
	let (first, second) = tokio::join!(
		fx.service.access_token(fx.user),
		fx.service.access_token(fx.user),
	);

	// Assert
	assert_eq!(first.unwrap().expose_secret(), "new-access");
	assert_eq!(second.unwrap().expose_secret(), "new-access");
}

#[rstest]
#[case::no_refresh_token(None)]
#[case::expired_refresh_token(Some(("old-refresh", Duration::seconds(-60))))]
#[tokio::test]
#[serial(database)]
async fn without_a_usable_refresh_token_the_user_must_sign_in_again(
	#[case] refresh: Option<(&str, Duration)>,
) {
	// Arrange
	let fx = fixture(database().await).await;
	fx.store(Duration::seconds(10), refresh).await;

	// Act
	let result = fx.service.access_token(fx.user).await;

	// Assert
	assert_eq!(
		result.err(),
		Some(TokenAccessError::ReauthenticationRequired)
	);
	assert_eq!(fx.refresh_calls().await, 0);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn a_user_who_never_signed_in_with_github_has_no_token() {
	// Arrange
	let fx = fixture(database().await).await;

	// Act
	let result = fx.service.access_token(fx.user).await;

	// Assert
	assert_eq!(result.err(), Some(TokenAccessError::NoToken));
}

#[rstest]
#[case::github_down(ResponseTemplate::new(503).set_body_string("PROVIDER-DETAIL"))]
#[case::unreadable(ResponseTemplate::new(200).set_body_string("not json"))]
#[case::lifetime_out_of_range(ResponseTemplate::new(200).set_body_json(json!({
	"access_token": "x", "token_type": "bearer", "expires_in": u64::MAX
})))]
#[tokio::test]
#[serial(database)]
async fn a_refresh_that_cannot_be_trusted_is_temporarily_unavailable(
	#[case] answer: ResponseTemplate,
) {
	// Arrange
	let fx = fixture(database().await).await;
	fx.store(
		Duration::seconds(10),
		Some(("old-refresh", Duration::days(30))),
	)
	.await;
	Mock::given(method("POST"))
		.and(path("/login/oauth/access_token"))
		.respond_with(answer)
		.mount(&fx.mock)
		.await;

	// Act
	let result = fx.service.access_token(fx.user).await;

	// Assert
	assert_eq!(result.err(), Some(TokenAccessError::Unavailable));
}
