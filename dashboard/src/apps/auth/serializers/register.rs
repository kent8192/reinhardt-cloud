//! Register request serializer.

use reinhardt::pages::ClientForm;
#[cfg(native)]
use reinhardt::{Schema, ToSchema};
use serde::{Deserialize, Serialize};

/// User registration request body.
#[reinhardt::dto]
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize, ClientForm)]
#[cfg_attr(native, derive(Schema))]
#[client_form(server_fn = crate::apps::auth::server_fn::register::register)]
pub struct RegisterRequest {
	#[validate(length(min = 3, max = 32))]
	pub username: String,
	#[validate(email, length(max = 254))]
	pub email: String,
	#[validate(length(min = 8, max = 128))]
	pub password: String,
}
