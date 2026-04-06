//! Tests for auth password hashing.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	fn test_password_hash_and_verify() {
		// Arrange
		use crate::apps::auth::models::User;
		use reinhardt::BaseUser;

		let mut user = User::new(
			"hashtest".to_string(),
			"hash@test.com".to_string(),
			String::new(),
			String::new(),
			None,
			true,
			false,
			false,
		);

		// Act
		user.set_password("secure-password-123").unwrap();

		// Assert
		assert!(user.password_hash().is_some());
		assert!(user.check_password("secure-password-123").unwrap());
		assert!(!user.check_password("wrong-password").unwrap());
	}
}
