//! Error types for the reinhardt-cloud operator reconciliation loop.

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

	/// The resource is missing a namespace.
	#[error("resource {0} has no namespace")]
	MissingNamespace(String),

	/// Failed to compute owner reference for a resource.
	#[error("failed to compute owner reference for {0}")]
	OwnerReference(String),

	/// A port number is outside the valid range (1-65535).
	#[error("invalid port {port} for field '{field}': must be between 1 and 65535")]
	InvalidPort { field: &'static str, port: i32 },

	/// Database provisioning failed.
	/// Used by the inference engine when database resource creation fails.
	#[allow(dead_code)]
	#[error("database provisioning failed: {0}")]
	DatabaseProvisioning(String),

	/// Platform controller is not installed in the cluster.
	/// Used by the inference engine when ACK/Config Connector CRDs are missing.
	#[allow(dead_code)]
	#[error("platform controller not installed: {group} API group not found")]
	PlatformControllerMissing { group: String },

	/// Secret generation failed.
	#[error("secret generation failed: {0}")]
	SecretGeneration(String),
}
