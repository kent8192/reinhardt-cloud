//! Tests for auth app.

pub mod e2e {
	pub mod test_auth_error_paths;
	pub mod test_email_verification;
	pub mod test_password_reset;
	pub mod test_register_login;
}
pub mod unit {
	pub mod test_auth_property;
	pub mod test_email_verification;
	pub mod test_jwt;
	pub mod test_oauth_backend;
	pub mod test_oauth_providers_view;
	pub mod test_oauth_settings;
	pub mod test_serializer_validation;
	pub mod test_session_service;
	pub mod test_social_account_model;
	pub mod test_token_service;
	pub mod test_user_model;
}
pub mod integration {
	pub mod test_credential_service;
	pub mod test_oauth_linking;
	pub mod test_oauth_storage;
}
