use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Background worker specification
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerSpec {
	/// Number of worker processes. Must be > 0.
	pub concurrency: Option<i32>,
	/// Custom entrypoint command for worker
	pub command: Option<Vec<String>>,
}

/// Status of worker deployment
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct WorkerStatus {
	pub ready_replicas: Option<i32>,
}

impl WorkerSpec {
	/// Validates the worker specification
	pub fn validate(&self) -> Result<(), String> {
		if let Some(c) = self.concurrency {
			if c <= 0 {
				return Err("worker.concurrency must be > 0".to_string());
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_worker_spec_valid_concurrency() {
		// Arrange
		let spec = WorkerSpec {
			concurrency: Some(4),
			command: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_worker_spec_rejects_zero_concurrency() {
		// Arrange
		let spec = WorkerSpec {
			concurrency: Some(0),
			command: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_worker_spec_rejects_negative_concurrency() {
		// Arrange
		let spec = WorkerSpec {
			concurrency: Some(-1),
			command: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_worker_spec_allows_none_concurrency() {
		// Arrange
		let spec = WorkerSpec {
			concurrency: None,
			command: Some(vec!["celery".to_string(), "worker".to_string()]),
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_ok());
	}
}
