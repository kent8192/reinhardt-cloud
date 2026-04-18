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

	/// Git credentials secret not found.
	/// Used by credential reconciliation when the referenced secret is missing.
	#[allow(dead_code)]
	#[error("git credentials secret '{0}' not found")]
	CredentialsMissing(String),

	/// Source build failed.
	/// Used by the build job reconciler when a Kaniko build fails.
	#[allow(dead_code)]
	#[error("source build failed: {0}")]
	BuildFailed(String),
}

/// Classification of reconciliation errors for backoff decisions.
///
/// The reconciler's error policy selects a requeue strategy based on the
/// error class. This lets us apply short backoffs for transient Kube API
/// flakes, longer backoffs when we are waiting on a dependency that is
/// not yet ready, and skip retries entirely for permanent failures where
/// a retry cannot succeed without user intervention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackoffClass {
	/// Transient error (API server hiccup, 5xx, etc.). Short exponential backoff.
	Transient,
	/// A dependency is not ready (404/409). Medium exponential backoff.
	DependencyNotReady,
	/// Permanent error (invalid spec). Do not retry until the object changes.
	Permanent,
}

impl BackoffClass {
	/// Short label used for metric/log cardinality.
	// Consumed by the Prometheus metrics exporter (see metrics module);
	// kept pub(crate) so the exporter can label counters consistently.
	#[allow(dead_code)]
	pub(crate) fn as_metric_label(self) -> &'static str {
		match self {
			BackoffClass::Transient => "transient",
			BackoffClass::DependencyNotReady => "dependency_not_ready",
			BackoffClass::Permanent => "permanent",
		}
	}
}

/// Classify a reconciliation error into a `BackoffClass`.
///
/// Heuristics:
/// - `MissingField`, `InvalidPort`: permanent — user must fix the spec.
/// - `Kube` with HTTP 404/409: dependency not ready (object missing or
///   write conflicts) — wait a bit longer before retrying.
/// - All other errors: transient — short backoff.
pub(crate) fn backoff_class(error: &Error) -> BackoffClass {
	match error {
		Error::MissingField(_) | Error::InvalidPort { .. } => BackoffClass::Permanent,
		Error::Kube(kube_err) => kube_status_class(kube_err),
		_ => BackoffClass::Transient,
	}
}

/// Extract the backoff class for a `kube::Error` by inspecting any
/// embedded HTTP status code. 404 and 409 indicate "not yet ready" cases
/// where a longer backoff is appropriate.
fn kube_status_class(err: &kube::Error) -> BackoffClass {
	if let kube::Error::Api(api_err) = err {
		match api_err.code {
			404 | 409 => return BackoffClass::DependencyNotReady,
			_ => {}
		}
	}
	BackoffClass::Transient
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn missing_field_is_permanent() {
		// Arrange
		let err = Error::MissingField("image");

		// Act
		let class = backoff_class(&err);

		// Assert
		assert_eq!(class, BackoffClass::Permanent);
	}

	#[rstest]
	fn invalid_port_is_permanent() {
		// Arrange
		let err = Error::InvalidPort {
			field: "port",
			port: 70_000,
		};

		// Act
		let class = backoff_class(&err);

		// Assert
		assert_eq!(class, BackoffClass::Permanent);
	}

	#[rstest]
	fn missing_namespace_is_transient() {
		// Arrange
		let err = Error::MissingNamespace("app".to_string());

		// Act
		let class = backoff_class(&err);

		// Assert
		assert_eq!(class, BackoffClass::Transient);
	}

	#[rstest]
	fn labels_are_stable() {
		// Assert: used as Prometheus label values, so keep stable.
		assert_eq!(BackoffClass::Transient.as_metric_label(), "transient");
		assert_eq!(
			BackoffClass::DependencyNotReady.as_metric_label(),
			"dependency_not_ready"
		);
		assert_eq!(BackoffClass::Permanent.as_metric_label(), "permanent");
	}
}
