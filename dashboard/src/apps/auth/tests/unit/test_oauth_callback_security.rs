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
}
