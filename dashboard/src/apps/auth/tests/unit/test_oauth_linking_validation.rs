//! Unit tests for `link_or_create_user`'s pre-database validation.
//!
//! `MissingClaim("sub")` short-circuits before any User-table or storage
//! call, so it can be exercised without a Postgres fixture. The
//! database-backed branches of the decision tree are covered in
//! `tests/integration/test_oauth_linking.rs`.

#[cfg(test)]
mod tests {
	use std::collections::HashMap;

	use reinhardt_auth::social::core::claims::StandardClaims;
	use reinhardt_auth::social::storage::InMemorySocialAccountStorage;
	use rstest::rstest;

	use crate::apps::auth::services::oauth::linking::{LinkError, link_or_create_user};

	fn empty_sub_claims() -> StandardClaims {
		StandardClaims {
			sub: String::new(),
			email: Some("alice@example.test".to_string()),
			email_verified: Some(true),
			name: Some("Alice".to_string()),
			given_name: None,
			family_name: None,
			picture: None,
			locale: None,
			additional_claims: HashMap::new(),
		}
	}

	#[rstest]
	#[tokio::test]
	async fn test_empty_sub_returns_missing_claim_error() {
		// Arrange — `sub` is the only field guaranteed to be present on
		// every OAuth provider per the OIDC spec; a missing/empty `sub`
		// means the provider violated the contract and we must refuse to
		// proceed (rather than e.g. matching every OAuth row with `sub=""`
		// to a single internal user).
		let storage = InMemorySocialAccountStorage::new();
		let claims = empty_sub_claims();

		// Act
		let result = link_or_create_user(&storage, "github", &claims, None).await;

		// Assert
		match result {
			Err(LinkError::MissingClaim(field)) => {
				assert_eq!(field, "sub");
			}
			other => panic!("expected MissingClaim(\"sub\"), got {other:?}"),
		}
	}

	#[rstest]
	fn test_email_conflict_display_includes_email_and_provider() {
		// Arrange — exercise the Display impl on the variant the callback
		// view maps to `AppError::Validation`. The user-facing message
		// must mention both the email and the provider so the UI can
		// guide the user to "sign in with your existing account first".
		let err = LinkError::EmailConflict {
			email: "alice@example.test".to_string(),
			provider: "github".to_string(),
		};

		// Act
		let rendered = err.to_string();

		// Assert
		assert!(
			rendered.contains("alice@example.test"),
			"EmailConflict message must include the conflicting email: {rendered}"
		);
		assert!(
			rendered.contains("github"),
			"EmailConflict message must name the provider: {rendered}"
		);
		assert!(
			rendered.contains("sign in with your existing account"),
			"EmailConflict message must guide the user to the existing-account path: {rendered}"
		);
	}
}
