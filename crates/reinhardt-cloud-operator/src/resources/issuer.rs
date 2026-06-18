//! Minimal local model of a cert-manager `Issuer` custom resource.
//!
//! cert-manager is a platform prerequisite. Rather than depend on an external
//! cert-manager crate, we model only the ACME + HTTP-01 shape the operator
//! emits, strongly typed via `#[derive(CustomResource)]` (CD-1).
//!
//! Workaround for reinhardt-cloud#707 (tracked in the same effort):
// The types below are dead in non-test builds until the preview-namespace
// builder in `resources::preview_namespace` (`build_issuer`) is wired in.
// Ideal implementation (without workaround): drop the `dead_code` allow once
// `build_issuer` constructs these types for the preview namespace reconciler.
#![allow(dead_code)]

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Operator-side model of `cert-manager.io/v1` `Issuer`.
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[kube(group = "cert-manager.io", version = "v1", kind = "Issuer", namespaced)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IssuerSpec {
	/// ACME configuration (Let's Encrypt etc.).
	pub(crate) acme: AcmeIssuer,
}

/// ACME block of a cert-manager `Issuer`.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AcmeIssuer {
	/// ACME directory endpoint.
	pub(crate) server: String,
	/// Registration email.
	pub(crate) email: String,
	/// Secret holding the ACME account private key.
	pub(crate) private_key_secret_ref: AcmeKeyRef,
	/// Challenge solvers.
	pub(crate) solvers: Vec<Http01Solver>,
}

/// Reference to the Secret storing the ACME account key.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub(crate) struct AcmeKeyRef {
	pub(crate) name: String,
}

/// HTTP-01 challenge solver.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Http01Solver {
	/// HTTP-01 ingress solver.
	pub(crate) http01: Http01IngressSolver,
}

/// Ingress-based HTTP-01 solver.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Http01IngressSolver {
	/// Ingress class used to solve HTTP-01 challenges.
	pub(crate) ingress: IngressClassRef,
}

/// Reference to an ingress class.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub(crate) struct IngressClassRef {
	/// Ingress class name, e.g. `nginx`.
	pub(crate) class: String,
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::Resource;
	use rstest::rstest;

	#[rstest]
	fn issuer_has_cert_manager_gvk() {
		// Act & Assert — `Resource` associated functions take `&DynamicType`
		// (which is `()` for `#[derive(CustomResource)]`).
		assert_eq!(Issuer::kind(&()).as_ref(), "Issuer");
		assert_eq!(Issuer::group(&()).as_ref(), "cert-manager.io");
		assert_eq!(Issuer::version(&()).as_ref(), "v1");
		assert_eq!(Issuer::api_version(&()).as_ref(), "cert-manager.io/v1");
	}

	#[rstest]
	fn issuer_roundtrip_preserves_acme_solver() {
		// Arrange
		let issuer = Issuer::new(
			"preview-issuer",
			IssuerSpec {
				acme: AcmeIssuer {
					server: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
					email: "ops@example.com".to_string(),
					private_key_secret_ref: AcmeKeyRef {
						name: "preview-acme-key".to_string(),
					},
					solvers: vec![Http01Solver {
						http01: Http01IngressSolver {
							ingress: IngressClassRef {
								class: "nginx".to_string(),
							},
						},
					}],
				},
			},
		);

		// Act
		let json = serde_json::to_string(&issuer).unwrap();
		let back: Issuer = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(back.metadata.name.as_deref(), Some("preview-issuer"));
		assert_eq!(back.spec.acme.email, "ops@example.com");
		assert_eq!(back.spec.acme.solvers[0].http01.ingress.class, "nginx");
		// camelCase casing for the renamed nested fields:
		assert!(json.contains("\"privateKeySecretRef\""));
		assert!(json.contains("\"http01\""));
		assert!(json.contains("\"apiVersion\":\"cert-manager.io/v1\""));
		assert!(json.contains("\"kind\":\"Issuer\""));
	}
}
