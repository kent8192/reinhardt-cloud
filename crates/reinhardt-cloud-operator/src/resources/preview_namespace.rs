//! Builders for the per-parent preview namespace triple (#707).
//!
//! For each parent `Project` with `spec.source.preview.enabled`, the operator
//! reconciles a deterministic `{parent}-preview` namespace plus a
//! `ResourceQuota` (from `PreviewBudget`), a `LimitRange`, a default-deny
//! `NetworkPolicy` with an ingress-controller + DNS allow policy, and a
//! cert-manager `Issuer`. These are pure builders modeled on `resources::tenant`.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{
	LimitRange, LimitRangeItem, LimitRangeSpec, Namespace, ResourceQuota, ResourceQuotaSpec,
};
use k8s_openapi::api::networking::v1::{
	NetworkPolicy, NetworkPolicyEgressRule, NetworkPolicyIngressRule, NetworkPolicyPeer,
	NetworkPolicyPort, NetworkPolicySpec,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use reinhardt_cloud_types::crd::source::PreviewBudget;

use crate::resources::issuer::{
	AcmeIssuer, AcmeKeyRef, Http01IngressSolver, Http01Solver, IngressClassRef, Issuer, IssuerSpec,
};

/// Managed-by label value for preview-namespace resources.
const MANAGED_BY_VALUE: &str = "reinhardt-cloud-operator";
/// Label recording the parent `Project` that owns a preview namespace.
pub(crate) const PARENT_LABEL_KEY: &str = "reinhardt.dev/parent-app";
/// Canonical namespace name for the NGINX ingress controller, used by the
/// allow policy so the cluster ingress path can reach preview Pods.
const INGRESS_CONTROLLER_NAMESPACE: &str = "ingress-nginx";

const QUOTA_NAME: &str = "preview-default-quota";
const LIMIT_RANGE_NAME: &str = "preview-default-limits";
const DEFAULT_DENY_NAME: &str = "preview-default-deny";
const ALLOW_INGRESS_NAME: &str = "preview-allow-ingress-and-dns";
/// Name of the cert-manager `Issuer` emitted into each preview namespace.
/// Referenced by the preview Ingress TLS annotation, so it is `pub(crate)`.
pub(crate) const ISSUER_NAME: &str = "preview-issuer";
/// Fallback pod cap applied when no `PreviewBudget` is set, so a runaway
/// preview cannot exhaust the cluster.
const DEFAULT_POD_CAP: &str = "50";
/// Default container CPU limit applied by the `LimitRange`.
const DEFAULT_LIMIT_CPU: &str = "500m";
/// Default container memory limit applied by the `LimitRange`.
const DEFAULT_LIMIT_MEMORY: &str = "512Mi";
/// Default container CPU request applied by the `LimitRange`.
const DEFAULT_REQUEST_CPU: &str = "100m";
/// Default container memory request applied by the `LimitRange`.
const DEFAULT_REQUEST_MEMORY: &str = "128Mi";

/// Returns the preview namespace name for a parent `Project`.
///
/// Format: `{parent_name}-preview` (e.g., `my-app-preview`).
pub(crate) fn preview_namespace_name(parent_name: &str) -> String {
	format!("{parent_name}-preview")
}

/// Standard labels applied to every resource in the preview namespace so
/// cleanup tooling can select them with a single label selector.
pub(crate) fn preview_namespace_labels(parent_name: &str) -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			"app.kubernetes.io/managed-by".to_string(),
			MANAGED_BY_VALUE.to_string(),
		),
		(PARENT_LABEL_KEY.to_string(), parent_name.to_string()),
		(
			"reinhardt.dev/preview-namespace".to_string(),
			"true".to_string(),
		),
	])
}

/// Builds the `{parent}-preview` `Namespace`.
pub(crate) fn build_namespace(parent_name: &str) -> Namespace {
	Namespace {
		metadata: ObjectMeta {
			name: Some(preview_namespace_name(parent_name)),
			labels: Some(preview_namespace_labels(parent_name)),
			..Default::default()
		},
		..Default::default()
	}
}

/// Builds the preview-namespace `ResourceQuota` from a `PreviewBudget`.
///
/// `max_cpu` -> `limits.cpu`, `max_memory` -> `limits.memory`. A pod-count cap
/// is always applied so a runaway preview cannot exhaust the cluster even when
/// no budget is declared.
pub(crate) fn build_resource_quota(
	parent_name: &str,
	budget: Option<&PreviewBudget>,
) -> ResourceQuota {
	let mut hard = BTreeMap::from([("pods".to_string(), Quantity(DEFAULT_POD_CAP.to_string()))]);
	if let Some(budget) = budget {
		if let Some(cpu) = &budget.max_cpu {
			hard.insert("limits.cpu".to_string(), Quantity(cpu.clone()));
		}
		if let Some(memory) = &budget.max_memory {
			hard.insert("limits.memory".to_string(), Quantity(memory.clone()));
		}
	}
	ResourceQuota {
		metadata: ObjectMeta {
			name: Some(QUOTA_NAME.to_string()),
			namespace: Some(preview_namespace_name(parent_name)),
			labels: Some(preview_namespace_labels(parent_name)),
			..Default::default()
		},
		spec: Some(ResourceQuotaSpec {
			hard: Some(hard),
			..Default::default()
		}),
		..Default::default()
	}
}

/// Builds a `LimitRange` giving preview containers default requests/limits
/// so Pods without explicit resource declarations get bounded defaults.
pub(crate) fn build_limit_range(parent_name: &str) -> LimitRange {
	LimitRange {
		metadata: ObjectMeta {
			name: Some(LIMIT_RANGE_NAME.to_string()),
			namespace: Some(preview_namespace_name(parent_name)),
			labels: Some(preview_namespace_labels(parent_name)),
			..Default::default()
		},
		spec: Some(LimitRangeSpec {
			limits: vec![LimitRangeItem {
				type_: "Container".to_string(),
				default: Some(BTreeMap::from([
					("cpu".to_string(), Quantity(DEFAULT_LIMIT_CPU.to_string())),
					(
						"memory".to_string(),
						Quantity(DEFAULT_LIMIT_MEMORY.to_string()),
					),
				])),
				default_request: Some(BTreeMap::from([
					("cpu".to_string(), Quantity(DEFAULT_REQUEST_CPU.to_string())),
					(
						"memory".to_string(),
						Quantity(DEFAULT_REQUEST_MEMORY.to_string()),
					),
				])),
				..Default::default()
			}],
		}),
	}
}

/// Builds the default-deny `NetworkPolicy` (deny all in/out) for the preview
/// namespace. Subsequent allow-policies layer on top of this baseline.
pub(crate) fn build_default_deny_policy(parent_name: &str) -> NetworkPolicy {
	NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(DEFAULT_DENY_NAME.to_string()),
			namespace: Some(preview_namespace_name(parent_name)),
			labels: Some(preview_namespace_labels(parent_name)),
			..Default::default()
		},
		spec: Some(NetworkPolicySpec {
			pod_selector: Some(LabelSelector::default()),
			policy_types: Some(vec!["Ingress".to_string(), "Egress".to_string()]),
			ingress: Some(vec![]),
			egress: Some(vec![]),
		}),
	}
}

/// Builds a `NetworkPolicy` allowing the NGINX ingress controller to reach
/// preview Pods (so the cluster ingress path works) and DNS egress on port 53
/// (so previews can resolve services). Mirrors the tenant ingress-controller
/// allow policy, scoped to the preview namespace.
pub(crate) fn build_allow_ingress_and_dns_policy(parent_name: &str) -> NetworkPolicy {
	NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(ALLOW_INGRESS_NAME.to_string()),
			namespace: Some(preview_namespace_name(parent_name)),
			labels: Some(preview_namespace_labels(parent_name)),
			..Default::default()
		},
		spec: Some(NetworkPolicySpec {
			pod_selector: Some(LabelSelector::default()),
			policy_types: Some(vec!["Ingress".to_string(), "Egress".to_string()]),
			ingress: Some(vec![NetworkPolicyIngressRule {
				from: Some(vec![NetworkPolicyPeer {
					namespace_selector: Some(LabelSelector {
						match_labels: Some(BTreeMap::from([(
							"kubernetes.io/metadata.name".to_string(),
							INGRESS_CONTROLLER_NAMESPACE.to_string(),
						)])),
						..Default::default()
					}),
					..Default::default()
				}]),
				..Default::default()
			}]),
			egress: Some(vec![NetworkPolicyEgressRule {
				ports: Some(vec![
					NetworkPolicyPort {
						port: Some(IntOrString::Int(53)),
						protocol: Some("UDP".to_string()),
						..Default::default()
					},
					NetworkPolicyPort {
						port: Some(IntOrString::Int(53)),
						protocol: Some("TCP".to_string()),
						..Default::default()
					},
				]),
				..Default::default()
			}]),
		}),
	}
}

/// Builds the cert-manager `Issuer` for the preview namespace, configured from
/// platform-level ACME settings (operator env config).
pub(crate) fn build_issuer(
	parent_name: &str,
	acme_server: &str,
	acme_email: &str,
	ingress_class: &str,
) -> Issuer {
	let mut issuer = Issuer::new(
		ISSUER_NAME,
		IssuerSpec {
			acme: AcmeIssuer {
				server: acme_server.to_string(),
				email: acme_email.to_string(),
				private_key_secret_ref: AcmeKeyRef {
					name: format!("{}-acme-key", preview_namespace_name(parent_name)),
				},
				solvers: vec![Http01Solver {
					http01: Http01IngressSolver {
						ingress: IngressClassRef {
							class: ingress_class.to_string(),
						},
					},
				}],
			},
		},
	);
	issuer.metadata.namespace = Some(preview_namespace_name(parent_name));
	issuer.metadata.labels = Some(preview_namespace_labels(parent_name));
	issuer
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn namespace_name_appends_preview_suffix() {
		// Arrange & Act
		let name = preview_namespace_name("my-app");

		// Assert
		assert_eq!(name, "my-app-preview");
	}

	#[rstest]
	fn labels_record_parent_and_managed_by() {
		// Arrange & Act
		let labels = preview_namespace_labels("my-app");

		// Assert
		assert_eq!(
			labels.get(PARENT_LABEL_KEY).map(String::as_str),
			Some("my-app")
		);
		assert_eq!(
			labels
				.get("app.kubernetes.io/managed-by")
				.map(String::as_str),
			Some(MANAGED_BY_VALUE)
		);
		assert_eq!(
			labels
				.get("reinhardt.dev/preview-namespace")
				.map(String::as_str),
			Some("true")
		);
	}

	#[rstest]
	fn resource_quota_maps_budget_to_hard_limits() {
		// Arrange
		let budget = PreviewBudget {
			max_replicas: Some(2),
			max_cpu: Some("2".to_string()),
			max_memory: Some("4Gi".to_string()),
		};

		// Act
		let quota = build_resource_quota("my-app", Some(&budget));
		let hard = quota.spec.expect("spec").hard.expect("hard limits");

		// Assert
		assert_eq!(hard.get("limits.cpu").map(|q| &q.0), Some(&"2".to_string()));
		assert_eq!(
			hard.get("limits.memory").map(|q| &q.0),
			Some(&"4Gi".to_string())
		);
		// Pod cap is always present.
		assert!(hard.contains_key("pods"));
		assert_eq!(quota.metadata.namespace.as_deref(), Some("my-app-preview"));
	}

	#[rstest]
	fn resource_quota_pod_only_when_no_budget() {
		// Act
		let quota = build_resource_quota("my-app", None);
		let hard = quota.spec.expect("spec").hard.expect("hard limits");

		// Assert
		assert!(hard.contains_key("pods"));
		assert!(!hard.contains_key("limits.cpu"));
		assert!(!hard.contains_key("limits.memory"));
	}

	#[rstest]
	fn allow_ingress_policy_targets_nginx_namespace_and_dns() {
		// Act
		let policy = build_allow_ingress_and_dns_policy("my-app");

		// Assert — ingress from the canonical ingress-nginx namespace.
		let spec = policy.spec.expect("spec");
		let from = spec.ingress.expect("ingress")[0]
			.from
			.clone()
			.expect("from");
		let ns_labels = from[0]
			.namespace_selector
			.as_ref()
			.expect("namespace_selector")
			.match_labels
			.as_ref()
			.expect("match_labels");
		assert_eq!(
			ns_labels
				.get("kubernetes.io/metadata.name")
				.map(String::as_str),
			Some(INGRESS_CONTROLLER_NAMESPACE)
		);
		// Assert — DNS egress on port 53 (UDP + TCP).
		let egress = spec.egress.expect("egress");
		let has_dns = egress.iter().any(|rule| {
			rule.ports
				.as_ref()
				.map(|ports| ports.iter().any(|p| p.port == Some(IntOrString::Int(53))))
				.unwrap_or(false)
		});
		assert!(has_dns, "expected DNS egress rule, got {egress:?}");
	}

	#[rstest]
	fn issuer_targets_preview_namespace() {
		// Act
		let issuer = build_issuer(
			"my-app",
			"https://acme.example/dir",
			"ops@example.com",
			"nginx",
		);

		// Assert
		assert_eq!(issuer.metadata.name.as_deref(), Some(ISSUER_NAME));
		assert_eq!(issuer.metadata.namespace.as_deref(), Some("my-app-preview"));
		assert_eq!(issuer.spec.acme.server, "https://acme.example/dir");
		assert_eq!(issuer.spec.acme.solvers[0].http01.ingress.class, "nginx");
	}
}
