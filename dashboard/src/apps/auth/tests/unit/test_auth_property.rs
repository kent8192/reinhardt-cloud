//! Property-based tests for auth components.

#[cfg(test)]
mod tests {
	use proptest::prelude::*;
	use reinhardt::BaseUser;

	use crate::apps::auth::models::User;

	proptest! {
		/// Any non-empty password should hash and verify correctly.
		#[test]
		fn test_password_hash_verify_roundtrip(
			password in ".{1,64}"
		) {
			// Arrange
			let mut user = User::build()
				.username("propuser".to_string())
				.email("prop@example.com".to_string())
				.first_name(String::new())
				.last_name(String::new())
				.password_hash(None)
				.is_active(true)
				.is_staff(false)
				.is_superuser(false)
				.finish();

			// Act
			user.set_password(&password).unwrap();

			// Assert
			prop_assert!(user.check_password(&password).unwrap());
		}
	}
}
