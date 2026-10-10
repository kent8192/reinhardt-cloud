//! Runtime regressions for contextual OAuth callback validation before token exchange.

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use reinhardt::auth::social::backend::SocialAuthBackend;
	use reinhardt::auth::social::core::{ProviderConfig, SocialAuthError};
	use reinhardt::auth::social::flow::{
		ContextualStateData, InMemoryStateStore, StateData, StateStore,
	};
	use reinhardt::auth::social::providers::github::GitHubProvider;
	use rstest::rstest;

	use crate::apps::auth::server_urls::oauth::oauth_state_binding;

	async fn backend(store: Arc<InMemoryStateStore>) -> SocialAuthBackend {
		let mut config = ProviderConfig::github(
			"test-client".to_owned(),
			"test-secret".to_owned(),
			"https://example.test/callback".to_owned(),
		);
		// An invalid callback must fail before attempting this local token endpoint.
		config.oauth2.as_mut().unwrap().token_endpoint = "http://127.0.0.1:9/token".to_owned();
		let mut backend = SocialAuthBackend::with_state_store(store);
		backend.register_provider(Arc::new(GitHubProvider::new(config).await.unwrap()));
		backend
	}

	#[rstest]
	#[case::other_browser("link.browser-b", Some("session-a"))]
	#[case::rotated_session("link.browser-a", Some("session-b"))]
	#[case::missing_session("link.browser-a", None)]
	#[case::removed_link_intent("browser-a", None)]
	#[tokio::test]
	async fn callback_rejects_swapped_binding_and_consumes_state(
		#[case] nonce: &str,
		#[case] session: Option<&str>,
	) {
		// Arrange
		let backend = backend(Arc::new(InMemoryStateStore::new())).await;
		let binding = oauth_state_binding("link.browser-a", Some("session-a")).unwrap();
		let authorization = backend
			.begin_auth_with_context("github", None, None, &binding, b"null".to_vec())
			.await
			.unwrap();
		let swapped_binding = oauth_state_binding(nonce, session).unwrap();

		// Act
		let swapped = backend
			.handle_callback_with_context("github", "code", &authorization.state, &swapped_binding)
			.await;
		let replay = backend
			.handle_callback_with_context("github", "code", &authorization.state, &binding)
			.await;

		// Assert
		assert_eq!(matches!(swapped, Err(SocialAuthError::InvalidState)), true);
		assert_eq!(matches!(replay, Err(SocialAuthError::InvalidState)), true);
	}

	#[rstest]
	#[case::other_provider("gitlab", false)]
	#[case::expired("github", true)]
	#[tokio::test]
	async fn callback_rejects_provider_swap_and_expired_state(
		#[case] provider: &str,
		#[case] expired: bool,
	) {
		// Arrange
		let store = Arc::new(InMemoryStateStore::new());
		let backend = backend(Arc::clone(&store)).await;
		let binding = oauth_state_binding("browser-a", None).unwrap();
		let mut state = StateData::new("state-a".to_owned(), None, None);
		if expired {
			state.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);
		}
		store
			.store_contextual(
				ContextualStateData::new(state, provider.to_owned(), &binding, b"null".to_vec())
					.unwrap(),
			)
			.await
			.unwrap();

		// Act
		let result = backend
			.handle_callback_with_context("github", "code", "state-a", &binding)
			.await;

		// Assert
		assert_eq!(matches!(result, Err(SocialAuthError::InvalidState)), true);
	}
}
