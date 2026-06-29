//! Tests for OAuth state cookie binding.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use crate::apps::auth::server_urls::{
		OAUTH_STATE_COOKIE_NAME, cookie_value_from_header, expired_oauth_state_cookie_header,
		oauth_state_cookie_header, oauth_state_cookie_signature,
	};

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
}
