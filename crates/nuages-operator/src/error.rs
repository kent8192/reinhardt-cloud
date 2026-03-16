//! Error types for the nuages operator reconciliation loop.

use thiserror::Error;

/// Errors that can occur during operator reconciliation.
#[derive(Debug, Error)]
pub(crate) enum Error {
	/// A Kubernetes API call failed.
	#[error("Kubernetes API error: {0}")]
	Kube(#[from] kube::Error),

	/// JSON serialization or deserialization failed.
	#[error("serialization error: {0}")]
	Serialization(#[from] serde_json::Error),

	/// Finalizer handling failed.
	#[error("finalizer error: {0}")]
	Finalizer(#[source] Box<dyn std::error::Error + Send + Sync>),

	/// A required field was missing from the resource spec.
	#[error("missing required field: {0}")]
	// Used by future reconciler validations for spec field checks
	#[allow(dead_code)]
	MissingField(&'static str),
}
