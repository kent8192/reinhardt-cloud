//! Tests for OAuth state cookie binding.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use crate::apps::auth::server_urls::oauth::{
		OAUTH_STATE_COOKIE_NAME, cookie_value_from_header, expired_oauth_state_cookie_header,
		oauth_link_intent_value, oauth_state_cookie_header, oauth_state_cookie_signature,
		validate_oauth_link_intent_value,
	};
	use uuid::Uuid;

	#[rstest]
	fn test_cookie_value_from_header_selects_named_cookie() {
		// Arrange
		let header = "sessionid=session-1; oauth_state_sig=signature-1; theme=dark";

		// Act
		let value = cookie_value_from_header(header, OAUTH_STATE_COOKIE_NAME);

		// Assert
		assert_eq!(value.as_deref(), Some("signature-1"));
	}

	#[rstest]
	fn test_oauth_state_cookie_signature_is_bound_to_provider_and_state() {
		// Arrange
		let secret = "test-secret";
		let signature = oauth_state_cookie_signature("github", "state-a", secret);

		// Act
		let other_provider = oauth_state_cookie_signature("gitlab", "state-a", secret);
		let other_state = oauth_state_cookie_signature("github", "state-b", secret);

		// Assert
		assert_ne!(signature, other_provider);
		assert_ne!(signature, other_state);
	}

	#[rstest]
	fn test_oauth_state_cookie_header_is_browser_bound_and_short_lived() {
		// Arrange
		let provider_id = "github";
		let state = "state-1";
		let secret = "test-secret";

		// Act
		let header = oauth_state_cookie_header(provider_id, state, secret, false);

		// Assert
		assert_eq!(
			header,
			format!(
				"oauth_state_sig={}; HttpOnly; SameSite=Lax; Path=/api/auth/oauth/github/callback/; Secure; Max-Age=600",
				oauth_state_cookie_signature(provider_id, state, secret)
			)
		);
	}

	#[rstest]
	fn test_expired_oauth_state_cookie_header_clears_matching_path() {
		// Arrange
		let debug = true;

		// Act
		let header = expired_oauth_state_cookie_header("github", debug);

		// Assert
		assert_eq!(
			header,
			"oauth_state_sig=; HttpOnly; SameSite=Lax; Path=/api/auth/oauth/github/callback/; Max-Age=0"
		);
	}

	#[rstest]
	fn account_link_intent_is_bound_to_user_provider_and_state() {
		// Arrange
		let user_id = Uuid::new_v4();
		let value = oauth_link_intent_value(
			"github",
			"state-a",
			user_id,
			"session-a",
			1_100,
			"test-secret",
		)
		.expect("account-link intent should serialize");

		// Act
		let accepted = validate_oauth_link_intent_value(
			&value,
			"github",
			"state-a",
			"session-a",
			"test-secret",
			1_000,
		);
		let different_provider = validate_oauth_link_intent_value(
			&value,
			"gitlab",
			"state-a",
			"session-a",
			"test-secret",
			1_000,
		);
		let different_state = validate_oauth_link_intent_value(
			&value,
			"github",
			"state-b",
			"session-a",
			"test-secret",
			1_000,
		);

		// Assert
		assert_eq!(accepted.expect("matching account-link intent"), user_id);
		assert!(different_provider.is_err());
		assert!(different_state.is_err());
	}

	#[rstest]
	fn account_link_intent_rejects_rotated_or_swapped_session() {
		// Arrange
		let user_id = Uuid::new_v4();
		let value = oauth_link_intent_value(
			"github",
			"state-a",
			user_id,
			"session-for-user-a",
			1_100,
			"test-secret",
		)
		.expect("account-link intent should serialize");

		// Act
		let rotated_session = validate_oauth_link_intent_value(
			&value,
			"github",
			"state-a",
			"rotated-session-for-user-a",
			"test-secret",
			1_000,
		);
		let swapped_session = validate_oauth_link_intent_value(
			&value,
			"github",
			"state-a",
			"session-for-user-b",
			"test-secret",
			1_000,
		);

		// Assert
		assert!(rotated_session.is_err());
		assert!(swapped_session.is_err());
	}

	#[rstest]
	fn expired_account_link_intent_is_rejected() {
		// Arrange
		let user_id = Uuid::new_v4();
		let value = oauth_link_intent_value(
			"github",
			"state-a",
			user_id,
			"session-a",
			1_000,
			"test-secret",
		)
		.expect("account-link intent should serialize");

		// Act
		let result = validate_oauth_link_intent_value(
			&value,
			"github",
			"state-a",
			"session-a",
			"test-secret",
			1_000,
		);

		// Assert
		assert!(result.is_err());
	}
}
