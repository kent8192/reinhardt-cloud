//! Tests for auth password hashing.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	fn test_password_hash_and_verify() {
		// Arrange
		use crate::apps::auth::models::User;
		use reinhardt::BaseUser;

		let mut user = User::build()
			.username("hashtest".to_string())
			.email("hash@test.com".to_string())
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(true)
			.is_staff(false)
			.is_superuser(false)
			.finish();

		// Act
		user.set_password("secure-password-123").unwrap();

		// Assert
		assert!(user.password_hash().is_some());
		assert!(user.check_password("secure-password-123").unwrap());
		assert!(!user.check_password("wrong-password").unwrap());
	}
}
