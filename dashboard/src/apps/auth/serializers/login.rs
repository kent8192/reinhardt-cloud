//! Login request serializer.

use reinhardt::dto;
use reinhardt::pages::client_form;
use serde::{Deserialize, Serialize};

/// Login request body.
#[dto(schema)]
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[client_form(server_fn = crate::apps::auth::server_fn::login::login, validate)]
pub struct LoginRequest {
	#[validate(length(min = 1, max = 150))]
	pub username: String,
	#[validate(length(min = 1, max = 128))]
	pub password: String,
}
