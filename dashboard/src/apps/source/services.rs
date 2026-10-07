//! Fixed source inputs and preview execution policy.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRevision {
	pub repository: String,
	pub commit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
	MutableRevision,
	ExternalFork,
}

impl SourceRevision {
	pub fn validate(&self, external_fork: bool) -> Result<(), SourceError> {
		if external_fork {
			return Err(SourceError::ExternalFork);
		}
		if !matches!(self.commit.len(), 40 | 64)
			|| !self.commit.bytes().all(|value| value.is_ascii_hexdigit())
		{
			return Err(SourceError::MutableRevision);
		}
		Ok(())
	}
}
