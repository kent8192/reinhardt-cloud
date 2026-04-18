//! View functions for auth endpoints.

use reinhardt::define_views;

define_views! {
	pub mod change_password;
	pub mod login;
	pub mod profile;
	pub mod register;
	pub mod verify_email;
}
