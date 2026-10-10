//! `User` has no password of any kind (SR-01).

use reinhardt::auth::BaseUser;
use rstest::rstest;

use crate::apps::accounts::models::User;

fn user() -> User {
	User::build()
		.github_user_id(1)
		.github_login("octocat".to_owned())
		.display_name("The Octocat".to_owned())
		.avatar_url(None)
		.email(None)
		.is_active(true)
		.is_staff(false)
		.last_login(None)
		.finish()
}

#[rstest]
#[case::empty("")]
#[case::the_login("octocat")]
#[case::something("correct horse battery staple")]
fn sr_01_no_password_ever_checks_out(#[case] attempt: &str) {
	// Arrange
	let user = user();

	// Act
	let result = user.check_password(attempt);

	// Assert
	assert!(!user.has_usable_password());
	assert_eq!(user.password_hash(), None);
	assert_eq!(result.ok(), Some(false));
}

#[rstest]
fn sr_01_a_password_cannot_be_set() {
	// Arrange
	let mut user = user();

	// Act
	let set = user.set_password("hunter2");
	user.set_password_hash("$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_owned());

	// Assert
	assert!(set.is_err(), "hashing is refused outright");
	assert!(!user.has_usable_password(), "a hash handed in is discarded");
	assert_eq!(user.password_hash(), None);
	assert_eq!(user.check_password("hunter2").ok(), Some(false));
}

#[rstest]
fn the_username_is_the_github_login_and_last_login_is_recorded() {
	// Arrange
	let mut user = user();
	let when = chrono::Utc::now();

	// Act
	BaseUser::set_last_login(&mut user, when);

	// Assert
	assert_eq!(User::get_username_field(), "github_login");
	assert_eq!(user.get_username(), "octocat");
	assert_eq!(BaseUser::last_login(&user), Some(when));
	assert!(BaseUser::is_active(&user));
}
