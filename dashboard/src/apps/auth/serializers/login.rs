//! Login request serializer.

use reinhardt::dto;
use reinhardt::pages::ClientForm;
#[cfg(server)]
use reinhardt::{Schema, ToSchema};
use serde::{Deserialize, Serialize};

/// Login request body.
#[dto]
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize, ClientForm)]
// Workaround for kent8192/reinhardt-web#6196 (tracked in reinhardt-cloud#894).
// Remove this workaround when `dto` can opt into generating `Schema` on server
// builds.
//
// Ideal implementation (without workaround):
//   #[dto] // generates the server-side `Schema` implementation
#[cfg_attr(server, derive(Schema))]
#[client_form(server_fn = crate::apps::auth::server_fn::login::login, validate)]
pub struct LoginRequest {
	#[validate(length(min = 1, max = 150))]
	pub username: String,
	#[validate(length(min = 1, max = 128))]
	pub password: String,
}
