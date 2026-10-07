//! Secret references contain identities, never confidential values.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretReference {
	pub environment_id: Uuid,
	pub version_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
	WrongEnvironment,
	Revoked,
}

impl SecretReference {
	pub fn validate_supply(&self, target: Uuid, revoked: bool) -> Result<(), SecretError> {
		if self.environment_id != target {
			return Err(SecretError::WrongEnvironment);
		}
		if revoked {
			return Err(SecretError::Revoked);
		}
		Ok(())
	}
}
