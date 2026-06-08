//! Tests for the account page rendering contract.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use crate::apps::auth::client::pages::account::render_account_content;
	use crate::apps::auth::server_fn::linked_accounts::LinkedOAuthAccountInfo;
	use crate::apps::auth::server_fn::oauth_providers::OAuthProviderInfo;
	use crate::shared::UserInfo;

	fn sample_user() -> UserInfo {
		UserInfo {
			id: "user-1".to_string(),
			username: "alice".to_string(),
			email: "alice@example.com".to_string(),
		}
	}

	#[rstest]
	fn unlinked_github_provider_renders_link_action() {
		// Arrange
		let providers = vec![OAuthProviderInfo {
			id: "github".to_string(),
			label: "GitHub".to_string(),
			start_url: "/api/auth/oauth/github/start/".to_string(),
		}];

		// Act
		let html = render_account_content(sample_user(), providers, Vec::new()).render_to_string();

		// Assert
		assert!(html.contains("Link GitHub"));
		assert!(html.contains(r#"href="/api/auth/oauth/github/start/""#));
		assert!(html.contains(r#"rel="external""#));
	}

	#[rstest]
	fn linked_github_provider_renders_status_without_link_action() {
		// Arrange
		let linked = vec![LinkedOAuthAccountInfo {
			provider: "github".to_string(),
			label: "GitHub".to_string(),
			provider_username: Some("octocat".to_string()),
		}];

		// Act
		let html = render_account_content(sample_user(), Vec::new(), linked).render_to_string();

		// Assert
		assert!(html.contains("GitHub account linked"));
		assert!(html.contains("octocat"));
		assert!(!html.contains("Link GitHub"));
	}
}
