//! User domain type for Reinhardt Cloud platform accounts.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A Reinhardt Cloud platform user account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
	pub id: Uuid,
	pub username: String,
	pub email: String,
	pub password_hash: String,
}

impl User {
	/// Creates a new user with a generated UUID.
	pub fn new(username: &str, email: &str, password_hash: &str) -> Self {
		Self {
			id: Uuid::new_v4(),
			username: username.to_string(),
			email: email.to_string(),
			password_hash: password_hash.to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_user_new_sets_fields() {
		// Arrange
		let username = "alice";
		let email = "alice@example.com";
		let hash = "$argon2id$v=19$m=19456,t=2,p=1$...";

		// Act
		let user = User::new(username, email, hash);

		// Assert
		assert_eq!(user.username, username);
		assert_eq!(user.email, email);
		assert_eq!(user.password_hash, hash);
	}
}
