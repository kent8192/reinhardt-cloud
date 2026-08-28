//! Register request serializer.

use reinhardt::dto;
use reinhardt::pages::ClientForm;
#[cfg(server)]
use reinhardt::{Schema, ToSchema};
use serde::{Deserialize, Serialize};

/// User registration request body.
#[dto]
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize, ClientForm)]
// Workaround for kent8192/reinhardt-web#6196 (tracked in reinhardt-cloud#894).
// Remove this workaround when `dto` can opt into generating `Schema` on server
// builds.
//
// Ideal implementation (without workaround):
//   #[dto] // generates the server-side `Schema` implementation
#[cfg_attr(server, derive(Schema))]
#[client_form(server_fn = crate::apps::auth::server_fn::register::register, validate)]
pub struct RegisterRequest {
	#[validate(length(min = 3, max = 32))]
	pub username: String,
	#[validate(email, length(max = 254))]
	pub email: String,
	#[validate(length(min = 8, max = 128))]
	pub password: String,
}
