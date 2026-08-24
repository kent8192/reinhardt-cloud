//! Regression tests for OAuth callback account-linking safety.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	fn oauth_callback_uses_signed_account_link_ownership_and_session_binding() {
		// Arrange
		let source = include_str!("../../server_urls.rs");

		// Act
		let callback_directly_extracts_ambient_cookie = source
			.split("pub async fn oauth_callback")
			.nth(1)
			.is_some_and(|callback| callback.contains("session_id_from_cookie_header"));
		let links_from_ambient_user =
			source.contains("link_or_create_user(&storage, &provider_id, &claims, current_user)");

		// Assert
		assert_eq!(callback_directly_extracts_ambient_cookie, false);
		assert_eq!(links_from_ambient_user, false);
		assert!(source.contains("let account_link_user_id = validate_oauth_state_cookie("));
		assert!(source.contains("&session_service,\n\t)\n\t.await?"));
		assert!(source.contains("current_user_for_account_link_intent(request, session_service)"));
		assert!(source.contains("&account_link_session.session_id"));
		assert!(source.contains("account_link_session.user.id != intent_user_id"));
		assert!(source.contains("oauth_link_session_binding(session_id, secret_key)"));
		assert!(source.contains("active_user_for_account_link_intent(user_id)"));
		assert!(
			source.contains("link_user_to_provider(&storage, &provider_id, &claims, intent_user)")
		);
		assert!(source.contains("link_or_create_user(&storage, &provider_id, &claims, None)"));
		assert!(source.contains("Response::temporary_redirect(if account_link_user_id.is_some()"));
		assert!(source.contains("\"/account\""));
	}

	#[rstest]
	fn oauth_start_sets_browser_bound_state_or_account_link_intent_cookie() {
		// Arrange
		let source = include_str!("../../server_urls.rs");
		let oauth_start = source
			.split("pub async fn oauth_start")
			.nth(1)
			.and_then(|start| {
				start
					.split("/// Complete an OAuth authorization flow")
					.next()
			})
			.expect("OAuth start route should be present");

		// Act
		let starts_backend_flow = oauth_start.contains(".begin_auth(&provider_id, None, None)");
		let sets_signed_state_cookie = oauth_start.contains("oauth_state_cookie_header(")
			&& oauth_start.contains("&auth.state,")
			&& oauth_start.contains("&settings.core.secret_key,");
		let sets_bound_link_intent = oauth_start.contains("oauth_link_intent_cookie_header(")
			&& oauth_start
				.contains("current_user_for_account_link_intent(&http_request, &session_service)")
			&& oauth_start.contains("Query(query): Query<OAuthStartQuery>");

		// Assert
		assert_eq!(starts_backend_flow, true);
		assert_eq!(sets_signed_state_cookie, true);
		assert_eq!(sets_bound_link_intent, true);
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
