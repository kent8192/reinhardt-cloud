//! `BaseUser` for `User`.
//!
//! `BaseUser` is what `CurrentUser<User>` and the admin site's user loader
//! require. A User signs in only through GitHub (SR-01), so the implementation
//! has no password at all: [`BaseUser::password_hash`] is always `None`, a hash
//! handed to [`BaseUser::set_password_hash`] is discarded, and the hasher
//! refuses to hash and never verifies. `check_password` is therefore `false`
//! for every input; there is no state in which a password could unlock a User.

use chrono::{DateTime, Utc};
use reinhardt::auth::{BaseUser, PasswordHasher};
use reinhardt::core::exception::Error;
use uuid::Uuid;

use crate::apps::accounts::models::User;

/// The hasher of a model that has no passwords: it hashes nothing and accepts
/// nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPasswordHasher;

impl PasswordHasher for NoPasswordHasher {
	fn hash(&self, _password: &str) -> Result<String, Error> {
		Err(Error::Authentication(
			"Users sign in with GitHub only; passwords are not supported".to_owned(),
		))
	}

	fn verify(&self, _password: &str, _hash: &str) -> Result<bool, Error> {
		Ok(false)
	}
}

impl BaseUser for User {
	type PrimaryKey = Uuid;
	type Hasher = NoPasswordHasher;

	fn get_username_field() -> &'static str {
		"github_login"
	}

	fn get_username(&self) -> &str {
		&self.github_login
	}

	fn password_hash(&self) -> Option<&str> {
		None
	}

	fn set_password_hash(&mut self, _hash: String) {
		// There is no column to hold a password hash, by design (SR-01): the
		// value is dropped rather than stored anywhere.
	}

	fn last_login(&self) -> Option<DateTime<Utc>> {
		self.last_login
	}

	fn set_last_login(&mut self, time: DateTime<Utc>) {
		self.last_login = Some(time);
	}

	fn is_active(&self) -> bool {
		self.is_active
	}
}
