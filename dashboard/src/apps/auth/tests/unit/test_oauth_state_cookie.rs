//! Tests for browser/session bindings and server-owned OAuth link context.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use uuid::Uuid;

	use crate::apps::auth::server_urls::oauth::{
		expired_oauth_state_cookie_header, oauth_account_link_user, oauth_state_binding,
		oauth_state_cookie_header,
	};

	#[rstest]
	fn oauth_binding_preserves_browser_and_session_boundaries() {
		// Arrange
		let binding = oauth_state_binding("link.browser-a", Some("session-a")).unwrap();

		// Act
		let swapped_browser = oauth_state_binding("link.browser-b", Some("session-a")).unwrap();
		let swapped_session = oauth_state_binding("link.browser-a", Some("session-b")).unwrap();
		let missing_session = oauth_state_binding("link.browser-a", None).unwrap();

		// Assert
		assert_eq!(binding, br#"["link.browser-a","session-a"]"#);
		assert_ne!(binding, swapped_browser);
		assert_ne!(binding, swapped_session);
		assert_ne!(binding, missing_session);
		assert_eq!(oauth_state_binding("", None).is_err(), true);
	}

	#[rstest]
	fn oauth_state_cookie_is_http_only_short_lived_and_contains_only_the_nonce() {
		// Arrange
		let nonce = "link.browser-a";

		// Act
		let header = oauth_state_cookie_header("github", nonce, false);

		// Assert
		assert_eq!(
			header,
			"oauth_state_sig=link.browser-a; HttpOnly; SameSite=Lax; Path=/api/auth/oauth/github/callback/; Secure; Max-Age=600"
		);
	}

	#[rstest]
	fn expired_oauth_cookie_clears_the_matching_path() {
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
	fn account_link_ownership_requires_matching_server_context_and_active_session_user() {
		// Arrange
		let user = Uuid::new_v4();
		let context = serde_json::to_vec(&Some(user)).unwrap();

		// Act
		let matching = oauth_account_link_user(&context, Some(user));
		let swapped = oauth_account_link_user(&context, Some(Uuid::new_v4()));
		let missing = oauth_account_link_user(&context, None);
		let ambient_login = oauth_account_link_user(b"null", Some(user));

		// Assert
		assert_eq!(matching.unwrap(), Some(user));
		assert_eq!(swapped.is_err(), true);
		assert_eq!(missing.is_err(), true);
		assert_eq!(ambient_login.is_err(), true);
		assert_eq!(oauth_account_link_user(b"null", None).unwrap(), None);
		assert_eq!(oauth_account_link_user(b"invalid", None).is_err(), true);
	}
}
