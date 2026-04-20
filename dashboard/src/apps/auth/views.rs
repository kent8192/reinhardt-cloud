//! View functions for auth endpoints.

use reinhardt::flatten_imports;

flatten_imports! {
	pub mod change_password;
	pub mod forgot_password;
	pub mod login;
	pub mod profile;
	pub mod register;
	pub mod reset_password;
	pub mod verify_email;
	pub mod verify_email_change;
}
