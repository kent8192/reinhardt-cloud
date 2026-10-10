//! Tests of the GitHub sign-in flow against a local stand-in for GitHub
//! (SR-04, SR-14, and the profile and membership lookups).

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use reinhardt::RedisSessionBackend;
use reinhardt::auth::StateStore;
use reinhardt::auth::social::flow::{ContextualStateData, StateData};
use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::middleware::session::AsyncSessionStateStore;
use reinhardt::test::fixtures::{ContainerAsync, GenericImage, redis_container};
use rstest::rstest;
use serde_json::json;
use sha2::{Digest, Sha256};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::apps::accounts::server::settings::GithubAppConfig;
use crate::apps::accounts::services::server::github::{
	CompleteError, GithubSignIn, STATE_KEY_PREFIX,
};
use crate::apps::accounts::services::server::sign_up_policy::{
	MembershipError, OrganizationMembership,
};
use crate::apps::accounts::tests::server_support::query_value;

const CODE: &str = "the-code";
const ACCESS: &str = "ghu_access_token";

struct Flow {
	github: GithubSignIn,
	mock: MockServer,
	redis_url: String,
	config: GithubAppConfig,
	_redis: ContainerAsync<GenericImage>,
}

async fn flow() -> Flow {
	let (redis, _port, redis_url) = redis_container().await;
	let mock = MockServer::start().await;
	let config = GithubAppConfig {
		client_id: "Iv1.test-client-id".to_owned(),
		client_secret: SecretString::new("test-client-secret"),
		redirect_uri: "http://127.0.0.1:1/api/auth/github/callback/".to_owned(),
		authorize_url: format!("{}/login/oauth/authorize", mock.uri()),
		token_url: format!("{}/login/oauth/access_token", mock.uri()),
		api_url: mock.uri(),
	};
	let github = GithubSignIn::new(&config, &SecretString::new(redis_url.clone()))
		.await
		.expect("the flow builds without a network");
	Flow {
		github,
		mock,
		redis_url,
		config,
		_redis: redis,
	}
}

impl Flow {
	async fn accept_the_code(&self) {
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.and(body_string_contains(format!("code={CODE}")))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"access_token": ACCESS,
				"token_type": "bearer",
				"expires_in": 28800,
				"refresh_token": "ghr_refresh",
				"refresh_token_expires_in": 15811200
			})))
			.mount(&self.mock)
			.await;
	}

	async fn serve_profile(&self, body: serde_json::Value) {
		Mock::given(method("GET"))
			.and(path("/user"))
			.respond_with(ResponseTemplate::new(200).set_body_json(body))
			.mount(&self.mock)
			.await;
	}

	async fn serve_octocat(&self) {
		self.serve_profile(json!({
			"id": 583231,
			"login": "octocat",
			"name": "The Octocat",
			"avatar_url": "https://avatars.example.test/u/583231"
		}))
		.await;
	}

	/// A second instance sharing this flow's Redis, as another replica would.
	async fn replica(&self) -> GithubSignIn {
		GithubSignIn::new(&self.config, &SecretString::new(self.redis_url.clone()))
			.await
			.expect("a replica builds")
	}

	/// A state store writing where the flow reads, to plant states by hand.
	fn state_store(&self) -> AsyncSessionStateStore<RedisSessionBackend> {
		let backend = RedisSessionBackend::new_from_url(&self.redis_url)
			.unwrap()
			.with_key_prefix(STATE_KEY_PREFIX.to_owned());
		AsyncSessionStateStore::new(backend)
	}
}

fn state_of(authorization_url: &str) -> String {
	query_value(authorization_url, "state").expect("the URL carries a state")
}

#[rstest]
#[tokio::test]
async fn sr_04_authorization_url_carries_s256_pkce_and_asks_for_no_scope() {
	// Arrange
	let flow = flow().await;

	// Act
	let started = flow.github.begin().await.unwrap();

	// Assert
	let url = &started.authorization_url;
	assert!(url.starts_with(&format!("{}/login/oauth/authorize?", flow.mock.uri())));
	assert_eq!(
		query_value(url, "code_challenge_method").as_deref(),
		Some("S256")
	);
	assert_eq!(
		query_value(url, "code_challenge").map(|v| v.len()),
		Some(43)
	);
	assert_eq!(
		query_value(url, "client_id").as_deref(),
		Some("Iv1.test-client-id")
	);
	assert_eq!(
		query_value(url, "scope"),
		None,
		"a GitHub App takes its permissions from the App, not from scopes"
	);
	assert!(state_of(url).len() >= 32);
}

#[rstest]
#[tokio::test]
async fn sr_04_the_binding_never_appears_in_the_url_and_differs_per_attempt() {
	// Arrange
	let flow = flow().await;

	// Act
	let first = flow.github.begin().await.unwrap();
	let second = flow.github.begin().await.unwrap();

	// Assert
	assert!(!first.authorization_url.contains(&first.binding));
	assert_ne!(first.binding, second.binding);
	assert_ne!(
		state_of(&first.authorization_url),
		state_of(&second.authorization_url)
	);
	assert_eq!(first.binding.len(), 43, "256 bits, base64url");
}

#[rstest]
#[tokio::test]
async fn sr_04_the_code_exchange_proves_possession_of_the_pkce_verifier() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();
	let challenge = query_value(&started.authorization_url, "code_challenge").unwrap();

	// Act
	let identity = flow
		.github
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await
		.unwrap();

	// Assert
	let exchange = flow
		.mock
		.received_requests()
		.await
		.unwrap()
		.into_iter()
		.find(|request| request.url.path() == "/login/oauth/access_token")
		.expect("the code was exchanged");
	let body = String::from_utf8(exchange.body).unwrap();
	let verifier = body
		.split('&')
		.find_map(|pair| pair.strip_prefix("code_verifier="))
		.expect("the exchange carries the verifier");
	assert_eq!(
		URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
		challenge
	);
	assert_eq!(identity.profile.github_user_id, 583_231);
}

#[rstest]
#[tokio::test]
async fn sr_04_a_state_can_be_used_only_once() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();
	let state = state_of(&started.authorization_url);

	// Act
	let first = flow
		.github
		.complete(CODE, &state, Some(&started.binding))
		.await;
	let replay = flow
		.github
		.complete(CODE, &state, Some(&started.binding))
		.await;

	// Assert
	assert!(first.is_ok());
	assert_eq!(replay.err(), Some(CompleteError::InvalidState));
}

#[rstest]
#[tokio::test]
async fn sr_04_concurrent_callbacks_with_one_state_succeed_exactly_once() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();
	let state = state_of(&started.authorization_url);
	let other = flow.replica().await;

	// Act
	let (here, there) = tokio::join!(
		flow.github.complete(CODE, &state, Some(&started.binding)),
		other.complete(CODE, &state, Some(&started.binding)),
	);

	// Assert
	assert_eq!(
		[here.is_ok(), there.is_ok()]
			.iter()
			.filter(|ok| **ok)
			.count(),
		1,
		"the state is consumed atomically across replicas"
	);
}

#[rstest]
#[tokio::test]
async fn sr_04_state_is_shared_across_replicas() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();
	let other = flow.replica().await;

	// Act
	let identity = other
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await;

	// Assert
	assert_eq!(identity.unwrap().profile.login, "octocat");
}

#[rstest]
#[tokio::test]
async fn sr_04_a_callback_from_another_browser_is_rejected_and_still_consumes_the_state() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();
	let state = state_of(&started.authorization_url);

	// Act
	let swapped = flow
		.github
		.complete(CODE, &state, Some("another-browsers-binding"))
		.await;
	let retry = flow
		.github
		.complete(CODE, &state, Some(&started.binding))
		.await;

	// Assert
	assert_eq!(swapped.err(), Some(CompleteError::InvalidState));
	assert_eq!(
		retry.err(),
		Some(CompleteError::InvalidState),
		"the rejected attempt used the state up"
	);
}

#[rstest]
#[case::absent(None)]
#[case::empty(Some(""))]
#[tokio::test]
async fn sr_04_a_callback_without_a_binding_cookie_is_rejected_and_still_consumes_the_state(
	#[case] binding: Option<&str>,
) {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();
	let state = state_of(&started.authorization_url);

	// Act
	let without = flow.github.complete(CODE, &state, binding).await;
	let retry = flow
		.github
		.complete(CODE, &state, Some(&started.binding))
		.await;

	// Assert
	assert_eq!(without.err(), Some(CompleteError::InvalidState));
	assert_eq!(retry.err(), Some(CompleteError::InvalidState));
}

#[rstest]
#[tokio::test]
async fn sr_04_a_state_issued_for_another_provider_is_rejected() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let state = "planted-state-for-another-provider";
	let binding = "browser-binding";
	let data = StateData::new(state.to_owned(), None, Some("verifier".to_owned()));
	let planted =
		ContextualStateData::new(data, "gitlab".to_owned(), binding.as_bytes(), vec![]).unwrap();
	flow.state_store().store_contextual(planted).await.unwrap();

	// Act
	let result = flow.github.complete(CODE, state, Some(binding)).await;

	// Assert
	assert_eq!(result.err(), Some(CompleteError::InvalidState));
}

#[rstest]
#[tokio::test]
async fn sr_04_a_state_expires() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let state = "short-lived-state";
	let binding = "browser-binding";
	let data = StateData::with_ttl(
		state.to_owned(),
		None,
		Some("verifier".to_owned()),
		chrono::Duration::seconds(2),
	);
	let planted =
		ContextualStateData::new(data, "github".to_owned(), binding.as_bytes(), vec![]).unwrap();
	flow.state_store().store_contextual(planted).await.unwrap();
	tokio::time::sleep(Duration::from_millis(2_600)).await;

	// Act
	let result = flow.github.complete(CODE, state, Some(binding)).await;

	// Assert
	assert_eq!(result.err(), Some(CompleteError::InvalidState));
}

#[rstest]
#[tokio::test]
async fn sr_04_the_state_lives_at_most_ten_minutes() {
	// Arrange
	let flow = flow().await;
	let started = flow.github.begin().await.unwrap();
	let client = redis::Client::open(flow.redis_url.as_str()).unwrap();
	let mut connection = client.get_multiplexed_async_connection().await.unwrap();

	// Act
	let key = format!(
		"{STATE_KEY_PREFIX}_social_auth_state:{}",
		state_of(&started.authorization_url)
	);
	let ttl: i64 = redis::cmd("TTL")
		.arg(&key)
		.query_async(&mut connection)
		.await
		.unwrap();

	// Assert
	assert!((1..=600).contains(&ttl), "the Redis TTL is {ttl} seconds");
}

#[rstest]
#[tokio::test]
async fn sr_04_a_declined_authorization_still_uses_the_state_up() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();
	let state = state_of(&started.authorization_url);

	// Act
	flow.github.discard_state(&state).await;
	let later = flow
		.github
		.complete(CODE, &state, Some(&started.binding))
		.await;

	// Assert
	assert_eq!(later.err(), Some(CompleteError::InvalidState));
}

#[rstest]
#[tokio::test]
async fn sr_02_the_login_comes_from_the_raw_profile_not_the_display_name() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();

	// Act
	let identity = flow
		.github
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await
		.unwrap();

	// Assert
	assert_eq!(identity.profile.github_user_id, 583_231);
	assert_eq!(identity.profile.login, "octocat");
	assert_eq!(identity.profile.name.as_deref(), Some("The Octocat"));
	assert_eq!(identity.profile.verified_email, None);
	assert_eq!(identity.tokens.access_token.expose_secret(), ACCESS);
	let lifetime = (identity.tokens.access_token_expires_at - Utc::now()).num_seconds();
	assert!((28_700..=28_800).contains(&lifetime), "{lifetime}");
	assert!(identity.tokens.refresh_token_expires_at.is_some());
}

#[rstest]
#[tokio::test]
async fn sr_02_a_failed_profile_call_is_a_failed_sign_in() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	Mock::given(method("GET"))
		.and(path("/user"))
		.respond_with(ResponseTemplate::new(500).set_body_string("PROVIDER-INTERNAL-DETAIL"))
		.mount(&flow.mock)
		.await;
	let started = flow.github.begin().await.unwrap();

	// Act
	let result = flow
		.github
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await;

	// Assert
	assert_eq!(result.err(), Some(CompleteError::ProfileUnavailable));
}

#[rstest]
#[tokio::test]
async fn sr_02_two_different_accounts_in_one_sign_in_are_refused() {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	Mock::given(method("GET"))
		.and(path("/user"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 1, "login": "first"})))
		.up_to_n_times(1)
		.mount(&flow.mock)
		.await;
	Mock::given(method("GET"))
		.and(path("/user"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 2, "login": "second"})))
		.mount(&flow.mock)
		.await;
	let started = flow.github.begin().await.unwrap();

	// Act
	let result = flow
		.github
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await;

	// Assert
	assert_eq!(result.err(), Some(CompleteError::ProfileUnavailable));
}

#[rstest]
#[case::unusable_id(json!({"id": 0, "login": "ghost"}))]
#[case::empty_login(json!({"id": 5, "login": ""}))]
#[tokio::test]
async fn sr_02_an_unusable_profile_is_a_failed_sign_in(#[case] profile: serde_json::Value) {
	// Arrange
	let flow = flow().await;
	flow.accept_the_code().await;
	flow.serve_profile(profile).await;
	let started = flow.github.begin().await.unwrap();

	// Act
	let result = flow
		.github
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await;

	// Assert
	assert_eq!(result.err(), Some(CompleteError::ProfileUnavailable));
}

#[rstest]
#[tokio::test]
async fn sr_14_provider_error_text_never_reaches_the_caller() {
	// Arrange
	let flow = flow().await;
	Mock::given(method("POST"))
		.and(path("/login/oauth/access_token"))
		.respond_with(
			ResponseTemplate::new(500)
				.set_body_string("PROVIDER-SECRET-DETAIL client_secret=hunter2"),
		)
		.mount(&flow.mock)
		.await;
	let started = flow.github.begin().await.unwrap();

	// Act
	let error = flow
		.github
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await
		.err()
		.expect("the exchange failed");

	// Assert
	assert_eq!(error, CompleteError::ExchangeFailed);
	for rendering in [
		error.to_string(),
		format!("{error:?}"),
		error.code().to_owned(),
	] {
		assert!(!rendering.contains("PROVIDER-SECRET-DETAIL"), "{rendering}");
		assert!(!rendering.contains("hunter2"), "{rendering}");
	}
}

#[rstest]
#[tokio::test]
async fn sr_14_an_error_in_a_ok_response_is_a_failed_exchange() {
	// Arrange
	let flow = flow().await;
	Mock::given(method("POST"))
		.and(path("/login/oauth/access_token"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!({
			"error": "bad_verification_code",
			"error_description": "The code passed is incorrect or expired."
		})))
		.mount(&flow.mock)
		.await;
	let started = flow.github.begin().await.unwrap();

	// Act
	let result = flow
		.github
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await;

	// Assert
	assert_eq!(result.err(), Some(CompleteError::ExchangeFailed));
}

#[rstest]
#[tokio::test]
async fn a_lifetime_outside_the_representable_range_fails_the_sign_in_without_panicking() {
	// Arrange
	let flow = flow().await;
	Mock::given(method("POST"))
		.and(path("/login/oauth/access_token"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!({
			"access_token": ACCESS,
			"token_type": "bearer",
			"expires_in": u64::MAX
		})))
		.mount(&flow.mock)
		.await;
	flow.serve_octocat().await;
	let started = flow.github.begin().await.unwrap();

	// Act
	let result = flow
		.github
		.complete(
			CODE,
			&state_of(&started.authorization_url),
			Some(&started.binding),
		)
		.await;

	// Assert
	assert_eq!(result.err(), Some(CompleteError::ExchangeFailed));
}

#[rstest]
#[tokio::test]
async fn sr_19_memberships_are_read_from_github_with_the_users_own_token() {
	// Arrange
	let flow = flow().await;
	Mock::given(method("GET"))
		.and(path("/user/orgs"))
		.and(wiremock::matchers::header(
			"authorization",
			"Bearer ghu_token",
		))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!([
			{"id": 7001, "login": "acme"},
			{"id": 7002, "login": "initech"}
		])))
		.mount(&flow.mock)
		.await;
	let membership = flow.github.memberships(SecretString::new("ghu_token"));

	// Act
	let ids = membership.organization_ids(1).await;

	// Assert
	assert_eq!(ids, Ok(vec![7001, 7002]));
}

#[rstest]
#[tokio::test]
async fn sr_19_memberships_follow_pagination() {
	// Arrange
	let flow = flow().await;
	let first_page: Vec<_> = (1..=100)
		.map(|id| json!({"id": id, "login": "o"}))
		.collect();
	Mock::given(method("GET"))
		.and(path("/user/orgs"))
		.and(wiremock::matchers::query_param("page", "1"))
		.respond_with(ResponseTemplate::new(200).set_body_json(first_page))
		.mount(&flow.mock)
		.await;
	Mock::given(method("GET"))
		.and(path("/user/orgs"))
		.and(wiremock::matchers::query_param("page", "2"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 101, "login": "o"}])))
		.mount(&flow.mock)
		.await;
	let membership = flow.github.memberships(SecretString::new("ghu_token"));

	// Act
	let ids = membership.organization_ids(1).await.unwrap();

	// Assert
	assert_eq!(ids.len(), 101);
	assert_eq!(ids.last(), Some(&101));
}

#[rstest]
#[case::github_error(ResponseTemplate::new(500).set_body_string("boom"))]
#[case::forbidden(ResponseTemplate::new(403))]
#[case::not_json(ResponseTemplate::new(200).set_body_string("<html>"))]
#[tokio::test]
async fn sr_19_a_membership_lookup_that_fails_is_an_error_not_an_empty_list(
	#[case] answer: ResponseTemplate,
) {
	// Arrange
	let flow = flow().await;
	Mock::given(method("GET"))
		.and(path("/user/orgs"))
		.respond_with(answer)
		.mount(&flow.mock)
		.await;
	let membership = flow.github.memberships(SecretString::new("ghu_token"));

	// Act
	let result = membership.organization_ids(1).await;

	// Assert
	assert_eq!(result, Err(MembershipError));
}

#[rstest]
#[tokio::test]
async fn sr_19_more_organizations_than_were_read_is_an_error() {
	// Arrange
	let flow = flow().await;
	let full_page: Vec<_> = (1..=100)
		.map(|id| json!({"id": id, "login": "o"}))
		.collect();
	Mock::given(method("GET"))
		.and(path("/user/orgs"))
		.respond_with(ResponseTemplate::new(200).set_body_json(full_page))
		.mount(&flow.mock)
		.await;
	let membership = flow.github.memberships(SecretString::new("ghu_token"));

	// Act
	let result = membership.organization_ids(1).await;

	// Assert
	assert_eq!(
		result,
		Err(MembershipError),
		"an incomplete answer must not admit anyone"
	);
}
