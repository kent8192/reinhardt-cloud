//! Register server function for frontend user creation.
//!
//! Creates a new user with `is_active = false` and sends a verification
//! email. The user must verify their email before they can log in.

#[cfg(native)]
use reinhardt::di::Depends;
#[cfg(native)]
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::pages::server_fn::{ServerFnError, server_fn};

#[cfg(native)]
use reinhardt::core::exception::{DatabaseErrorKind, Error as AppError};

#[cfg(native)]
use crate::apps::auth::models::User;
use crate::apps::auth::serializers::RegisterRequest;
#[cfg(native)]
use crate::apps::auth::services::EmailService;
#[cfg(native)]
use crate::config::ProjectSettings;
use crate::shared::AuthResponse;

/// Create a new user account with email verification.
///
/// On the server side this creates a new user in the database with a
/// hashed password and `is_active = false`, then sends a verification
/// email. No session cookie is set — the user must verify their email
/// first. Proven username/email uniqueness violations return structured field errors.
#[server_fn]
pub async fn register(
	request: RegisterRequest,
	#[inject] _http_request: ServerFnRequest,
	#[inject] settings: Depends<ProjectSettings>,
	#[inject] email_service: Depends<EmailService>,
) -> Result<AuthResponse, ServerFnError> {
	use crate::apps::auth::services;
	use crate::shared::UserInfo;

	let request = request.normalized();
	reinhardt::Validate::validate(&request).map_err(ServerFnError::from)?;

	let created = services::register_inactive_user(
		&request.username,
		&request.email,
		&request.password,
		email_service.as_ref(),
		settings.as_ref(),
	)
	.await
	.map_err(server_fn_error_from_app_error)?;

	// No session cookie — user must verify email first
	let user_info = UserInfo::from(&created);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
	})
}

#[cfg(native)]
fn server_fn_error_from_app_error(err: AppError) -> ServerFnError {
	let err =
		match ServerFnError::try_from_model_error_with::<User, _>(err, |error, fields| {
			match (error.kind(), fields) {
				(DatabaseErrorKind::UniqueViolation, ["email"]) => {
					Some("Email already exists".to_owned())
				}
				(DatabaseErrorKind::UniqueViolation, ["username"]) => {
					Some("Username already exists".to_owned())
				}
				_ => None,
			}
		}) {
			Ok(error) => return error,
			Err(error) => error,
		};
	match err {
		AppError::Authentication(message)
		| AppError::Conflict(message)
		| AppError::Validation(message)
		| AppError::Http(message) => ServerFnError::application(message),
		AppError::Internal(message) => ServerFnError::application(message),
		_ => {
			tracing::error!("Registration failed: {err}");
			ServerFnError::application("Internal server error")
		}
	}
}

#[cfg(all(test, native))]
mod tests {
	use std::collections::BTreeSet;

	use reinhardt::core::exception::DatabaseError;
	use reinhardt::db::orm::Model;
	use reinhardt::pages::server_fn::ServerFnErrorKind;
	use rstest::rstest;

	use super::*;

	#[rstest]
	#[case("email", "Email already exists")]
	#[case("username", "Username already exists")]
	fn registration_maps_known_unique_fields_without_driver_text(
		#[case] field: &str,
		#[case] message: &str,
	) {
		// Arrange
		let error = DatabaseError::new(DatabaseErrorKind::UniqueViolation, "private diagnostic")
			.with_table(User::table_name())
			.with_columns([field]);

		// Act
		let error = server_fn_error_from_app_error(AppError::from(error));

		// Assert
		assert_eq!(
			error,
			ServerFnError::validation_with_message(message, [(field, message)])
		);
	}

	#[rstest]
	#[case("email", "auth_users_email_uniq_61d7d628", "Email already exists")]
	#[case(
		"username",
		"auth_users_username_uniq_8ea19568",
		"Username already exists"
	)]
	fn registration_maps_existing_migration_constraint_names(
		#[case] field: &str,
		#[case] constraint: &str,
		#[case] message: &str,
	) {
		// Arrange
		let error = DatabaseError::new(DatabaseErrorKind::UniqueViolation, "private diagnostic")
			.with_table(User::table_name())
			.with_constraint(constraint);

		// Act
		let error = server_fn_error_from_app_error(AppError::from(error));

		// Assert
		assert_eq!(
			error,
			ServerFnError::validation_with_message(message, [(field, message)])
		);
	}

	#[rstest]
	#[case(DatabaseError::new(DatabaseErrorKind::UniqueViolation, "private duplicate email"))]
	#[case(DatabaseError::new(DatabaseErrorKind::Query, "duplicate key (email)=private"))]
	#[case(DatabaseError::new(DatabaseErrorKind::UniqueViolation, "private")
		.with_table("another_table").with_columns(["email"]))]
	#[case(DatabaseError::new(DatabaseErrorKind::UniqueViolation, "private")
		.with_constraint("unknown_constraint").with_columns(["email"]))]
	fn registration_keeps_unmapped_database_errors_private(#[case] error: DatabaseError) {
		// Arrange
		let error = AppError::from(error);

		// Act
		let error = server_fn_error_from_app_error(error);

		// Assert
		assert_eq!(error, ServerFnError::application("Internal server error"));
	}

	#[rstest]
	fn register_request_validation_preserves_structured_field_errors() {
		// Arrange
		let request = RegisterRequest {
			username: "ab".to_string(),
			email: "invalid".to_string(),
			password: "short".to_string(),
		};

		// Act
		let error = reinhardt::Validate::validate(&request)
			.map_err(ServerFnError::from)
			.expect_err("invalid registration request should not reach registration service");

		// Assert
		assert_eq!(error.kind(), ServerFnErrorKind::Validation);
		assert_eq!(
			error
				.field_errors()
				.iter()
				.map(|field| field.field())
				.collect::<BTreeSet<_>>(),
			BTreeSet::from(["email", "password", "username"])
		);
	}
}
