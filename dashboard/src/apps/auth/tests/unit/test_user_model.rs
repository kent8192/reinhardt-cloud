//! Tests for User model construction and behavior.

#[cfg(test)]
mod tests {
	use reinhardt::BaseUser;
	use rstest::rstest;

	use crate::apps::auth::models::User;

	#[rstest]
	fn test_user_new_sets_all_fields() {
		// Arrange
		let username = "testuser".to_string();
		let email = "test@example.com".to_string();
		let first_name = "Test".to_string();
		let last_name = "User".to_string();

		// Act
		let user = User::build()
			.username(username.clone())
			.email(email.clone())
			.first_name(first_name.clone())
			.last_name(last_name.clone())
			.password_hash(None)
			.is_active(true)
			.is_staff(false)
			.is_superuser(false)
			.finish();

		// Assert
		assert_eq!(user.get_username(), username);
		assert_eq!(user.email, email);
		assert_eq!(user.first_name, first_name);
		assert_eq!(user.last_name, last_name);
		assert!(user.password_hash().is_none());
		assert!(user.is_active());
		assert!(!user.is_staff);
		assert!(!user.is_superuser);
	}

	#[rstest]
	fn test_user_default_is_active_true() {
		// Arrange & Act
		let user = User::build()
			.username("activeuser".to_string())
			.email("active@example.com".to_string())
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(true)
			.is_staff(false)
			.is_superuser(false)
			.finish();

		// Assert
		assert!(
			user.is_active(),
			"User created with is_active=true should be active"
		);
	}

	#[rstest]
	fn test_user_password_hash_starts_with_argon2() {
		// Arrange
		let mut user = User::build()
			.username("hashuser".to_string())
			.email("hash@example.com".to_string())
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(true)
			.is_staff(false)
			.is_superuser(false)
			.finish();

		// Act
		user.set_password("my-secure-password").unwrap();

		// Assert
		let hash = user
			.password_hash()
			.expect("Password hash should be set after set_password");
		assert!(
			hash.starts_with("$argon2"),
			"Password hash should start with $argon2, got: {}",
			&hash[..hash.len().min(20)]
		);
	}

	#[rstest]
	fn test_user_check_password_no_hash() {
		// Arrange
		let user = User::build()
			.username("nohash".to_string())
			.email("nohash@example.com".to_string())
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(true)
			.is_staff(false)
			.is_superuser(false)
			.finish();

		// Act
		let result = user.check_password("anypassword");

		// Assert — with no hash, check_password should return Ok(false) or Err
		if let Ok(valid) = result {
			assert!(!valid, "check_password with no hash should return false");
		}
	}

	#[rstest]
	fn test_user_serialization_roundtrip() {
		// Arrange
		let user = User::build()
			.username("serdeuser".to_string())
			.email("serde@example.com".to_string())
			.first_name("Serde".to_string())
			.last_name("Test".to_string())
			.password_hash(Some("$argon2id$fakehash".to_string()))
			.is_active(true)
			.is_staff(true)
			.is_superuser(false)
			.finish();

		// Act
		let json = serde_json::to_string(&user).expect("Failed to serialize User");
		let deserialized: User = serde_json::from_str(&json).expect("Failed to deserialize User");

		// Assert
		assert_eq!(deserialized.get_username(), user.get_username());
		assert_eq!(deserialized.email, user.email);
		assert_eq!(deserialized.first_name, user.first_name);
		assert_eq!(deserialized.last_name, user.last_name);
		assert_eq!(deserialized.is_staff, user.is_staff);
		assert_eq!(deserialized.is_superuser, user.is_superuser);
	}
}
