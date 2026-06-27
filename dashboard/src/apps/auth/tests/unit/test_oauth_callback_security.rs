//! Regression tests for OAuth callback account-linking safety.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	fn oauth_callback_does_not_link_from_ambient_session_cookie() {
		// Arrange
		let source = include_str!("../../server_urls.rs");

		// Act
		let reads_ambient_cookie = source.contains("current_user_from_cookie")
			|| source.contains("session_id_from_cookie_header");
		let links_from_ambient_user =
			source.contains("link_or_create_user(&storage, &provider_id, &claims, current_user)");

		// Assert
		assert_eq!(reads_ambient_cookie, false);
		assert_eq!(links_from_ambient_user, false);
		assert!(source.contains("link_or_create_user(&storage, &provider_id, &claims, None)"));
	}

	#[rstest]
	fn oauth_start_sets_browser_bound_state_cookie() {
		// Arrange
		let source = include_str!("../../server_urls.rs");

		// Act
		let starts_backend_flow = source.contains(".begin_auth(&provider_id, None, None)");
		let sets_signed_state_cookie = source.contains("oauth_state_cookie_header(")
			&& source.contains("&provider_id,\n\t\t\t\t&auth.state,")
			&& source.contains("&settings.core.secret_key,");

		// Assert
		assert_eq!(starts_backend_flow, true);
		assert_eq!(sets_signed_state_cookie, true);
		assert!(source.contains(
			"pub(in crate::apps::auth) const OAUTH_STATE_COOKIE_NAME: &str = \"oauth_state_sig\";"
		));
	}

	#[rstest]
	fn oauth_callback_requires_matching_state_cookie_before_backend_callback() {
		// Arrange
		let source = include_str!("../../server_urls.rs");
		let cookie_check = source
			.find("validate_oauth_state_cookie(\n\t\t&http_request,")
			.expect("OAuth callback should validate a browser-bound state cookie");
		let callback = source
			.find(".handle_callback(&provider_id, &query.code, &query.state)")
			.expect("OAuth callback should still validate provider state through backend");

		// Act
		let checks_before_backend_callback = cookie_check < callback;
		let rejects_mismatch = source.contains("OAuth state mismatch");
		let clears_state_cookie = source.contains("expired_oauth_state_cookie_header(");

		// Assert
		assert_eq!(checks_before_backend_callback, true);
		assert_eq!(rejects_mismatch, true);
		assert_eq!(clears_state_cookie, true);
	}
}
