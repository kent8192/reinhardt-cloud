//! Login request serializer.

use reinhardt::pages::ClientForm;
#[cfg(server)]
use reinhardt::{Schema, ToSchema};
use serde::{Deserialize, Serialize};

/// Login request body.
#[reinhardt::dto]
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize, ClientForm)]
#[cfg_attr(server, derive(Schema))]
#[client_form(server_fn = crate::apps::auth::server_fn::login::login, validate)]
pub struct LoginRequest {
	#[validate(length(min = 1, max = 150))]
	pub username: String,
	#[validate(length(min = 1, max = 128))]
	pub password: String,
}
