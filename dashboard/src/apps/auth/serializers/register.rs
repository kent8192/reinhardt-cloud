//! Register request serializer.

use reinhardt::dto;
use reinhardt::pages::client_form;
use serde::{Deserialize, Serialize};

/// User registration request body.
#[dto(schema)]
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[client_form(server_fn = crate::apps::auth::server_fn::register::register, validate)]
pub struct RegisterRequest {
	#[validate(length(min = 3, max = 32))]
	pub username: String,
	#[validate(email, length(max = 254))]
	pub email: String,
	#[validate(length(min = 8, max = 128))]
	pub password: String,
}

impl RegisterRequest {
	/// Normalize user-entered text before applying DTO validation.
	pub(crate) fn normalized(mut self) -> Self {
		self.username = self.username.trim().to_owned();
		self.email = self.email.trim().to_lowercase();
		self
	}
}

impl RegisterRequestClientForm {
	/// Normalize bound values before generated client validation and dispatch.
	pub(crate) fn normalize_values(runtime: &reinhardt::pages::UseFormReturn<Self>) {
		let request = Self::to_request(runtime).normalized();
		runtime.set_value(RegisterRequestClientFormField::Username, request.username);
		runtime.set_value(RegisterRequestClientFormField::Email, request.email);
	}
}
