//! Builders for the per-parent preview namespace triple (#707).
//!
//! For each parent `Project` with `spec.source.preview.enabled`, the operator
//! reconciles a deterministic namespace keyed by the parent namespace and name plus a
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
/// Label recording the namespace of the parent `Project` that owns a preview namespace.
pub(crate) const PARENT_NAMESPACE_LABEL_KEY: &str = "reinhardt.dev/parent-namespace";
/// Label recording the Kubernetes UID of the parent `Project` that owns a preview namespace.
pub(crate) const PARENT_UID_LABEL_KEY: &str = "reinhardt.dev/parent-uid";
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

/// Maximum length of a Kubernetes DNS-1123 label.
const DNS_1123_LABEL_MAX_LENGTH: usize = 63;

/// Suffix used for preview namespaces.
const PREVIEW_NAMESPACE_SUFFIX: &str = "preview";

/// Number of lowercase hexadecimal FNV-1a characters appended for collision resistance.
const IDENTITY_HASH_LENGTH: usize = 12;

fn trim_dns_label_prefix(value: &str, max_len: usize) -> &str {
	let mut end = value.len().min(max_len);
	while end > 0 && !value.is_char_boundary(end) {
		end -= 1;
	}
	value[..end].trim_end_matches('-')
}

fn sanitize_dns_label_component(value: &str) -> String {
	let mut output = String::with_capacity(value.len());
	let mut previous_dash = false;
	for ch in value.chars() {
		let mapped = if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
			ch
		} else {
			'-'
		};
		if mapped == '-' {
			if !previous_dash {
				output.push('-');
			}
			previous_dash = true;
		} else {
			output.push(mapped);
			previous_dash = false;
		}
	}
	output.trim_matches('-').to_string()
}

fn identity_hash(parent_namespace: &str, parent_name: &str) -> String {
	let mut hash = 0xcbf29ce484222325_u64;
	for byte in parent_namespace
		.bytes()
		.chain([0])
		.chain(parent_name.bytes())
	{
		hash ^= u64::from(byte);
		hash = hash.wrapping_mul(0x100000001b3);
	}
	format!("{hash:016x}")[..IDENTITY_HASH_LENGTH].to_string()
}

/// Returns the preview namespace name for a parent `Project`.
///
/// The parent `Project` is namespaced, but Kubernetes `Namespace` is cluster-scoped.
/// Including the parent namespace in the namespace identity prevents tenants with
/// the same `Project` name from sharing preview guardrails or cleanup targets.
/// The parent `Project` name is normalized because CRD object names may include
/// dots, while `Namespace` names must be DNS-1123 labels.
pub(crate) fn preview_namespace_name(parent_namespace: &str, parent_name: &str) -> String {
	let safe_parent_name = sanitize_dns_label_component(parent_name);
	let identity = format!("{parent_namespace}-{safe_parent_name}");
	let hash = identity_hash(parent_namespace, parent_name);
	let suffix_len = 1 + IDENTITY_HASH_LENGTH + 1 + PREVIEW_NAMESPACE_SUFFIX.len();
	let prefix_len = DNS_1123_LABEL_MAX_LENGTH - suffix_len;
	let prefix = trim_dns_label_prefix(&identity, prefix_len);
	format!("{prefix}-{hash}-{PREVIEW_NAMESPACE_SUFFIX}")
}

pub(crate) fn legacy_preview_namespace_matches(preview_namespace: &str, parent_name: &str) -> bool {
	let safe_parent_name = sanitize_dns_label_component(parent_name);
	if safe_parent_name.is_empty() {
		return false;
	}
	let Some((identity_prefix, _hash)) = legacy_preview_namespace_parts(preview_namespace) else {
		return false;
	};

	let parent_suffix = format!("-{safe_parent_name}");
	if let Some(parent_namespace) = identity_prefix.strip_suffix(&parent_suffix)
		&& !parent_namespace.is_empty()
	{
		return preview_namespace_name(parent_namespace, parent_name) == preview_namespace;
	}

	false
}

fn legacy_preview_namespace_parts(preview_namespace: &str) -> Option<(&str, &str)> {
	let namespace_without_suffix =
		preview_namespace.strip_suffix(&format!("-{PREVIEW_NAMESPACE_SUFFIX}"))?;
	let (identity_prefix, hash) = namespace_without_suffix.rsplit_once('-')?;
	if hash.len() != IDENTITY_HASH_LENGTH
		|| !hash
			.chars()
			.all(|character| matches!(character, '0'..='9' | 'a'..='f'))
	{
		return None;
	}

	Some((identity_prefix, hash))
}

/// Standard labels applied to every resource in the preview namespace so
/// cleanup tooling can select them with a single label selector.
pub(crate) fn preview_namespace_labels(
	parent_namespace: &str,
	parent_name: &str,
) -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			"app.kubernetes.io/managed-by".to_string(),
			MANAGED_BY_VALUE.to_string(),
		),
		(PARENT_LABEL_KEY.to_string(), parent_name.to_string()),
		(
			PARENT_NAMESPACE_LABEL_KEY.to_string(),
			parent_namespace.to_string(),
		),
		(
			"reinhardt.dev/preview-namespace".to_string(),
			"true".to_string(),
		),
	])
}

/// Standard labels applied to a preview namespace, including the parent
/// identity required before destructive cleanup.
pub(crate) fn preview_namespace_owner_labels(
	parent_namespace: &str,
	parent_name: &str,
	parent_uid: &str,
) -> BTreeMap<String, String> {
	let mut labels = preview_namespace_labels(parent_namespace, parent_name);
	labels.insert(PARENT_UID_LABEL_KEY.to_string(), parent_uid.to_string());
	labels
}

/// Returns whether namespace labels prove ownership by the exact parent
/// `Project` instance.
pub(crate) fn labels_match_preview_owner(
	labels: Option<&BTreeMap<String, String>>,
	parent_namespace: &str,
	parent_name: &str,
	parent_uid: &str,
) -> bool {
	let Some(labels) = labels else {
		return false;
	};
	labels
		.get("app.kubernetes.io/managed-by")
		.is_some_and(|value| value == MANAGED_BY_VALUE)
		&& labels
			.get("reinhardt.dev/preview-namespace")
			.is_some_and(|value| value == "true")
		&& labels
			.get(PARENT_LABEL_KEY)
			.is_some_and(|value| value == parent_name)
		&& labels
			.get(PARENT_NAMESPACE_LABEL_KEY)
			.is_some_and(|value| value == parent_namespace)
		&& labels
			.get(PARENT_UID_LABEL_KEY)
			.is_some_and(|value| value == parent_uid)
}

/// Builds the parent-qualified preview `Namespace`.
pub(crate) fn build_namespace(
	parent_namespace: &str,
	parent_name: &str,
	parent_uid: &str,
) -> Namespace {
	Namespace {
		metadata: ObjectMeta {
			name: Some(preview_namespace_name(parent_namespace, parent_name)),
			labels: Some(preview_namespace_owner_labels(
				parent_namespace,
				parent_name,
				parent_uid,
			)),
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
	parent_namespace: &str,
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
			namespace: Some(preview_namespace_name(parent_namespace, parent_name)),
			labels: Some(preview_namespace_labels(parent_namespace, parent_name)),
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
pub(crate) fn build_limit_range(parent_namespace: &str, parent_name: &str) -> LimitRange {
	LimitRange {
		metadata: ObjectMeta {
			name: Some(LIMIT_RANGE_NAME.to_string()),
			namespace: Some(preview_namespace_name(parent_namespace, parent_name)),
			labels: Some(preview_namespace_labels(parent_namespace, parent_name)),
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
pub(crate) fn build_default_deny_policy(
	parent_namespace: &str,
	parent_name: &str,
) -> NetworkPolicy {
	NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(DEFAULT_DENY_NAME.to_string()),
			namespace: Some(preview_namespace_name(parent_namespace, parent_name)),
			labels: Some(preview_namespace_labels(parent_namespace, parent_name)),
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
pub(crate) fn build_allow_ingress_and_dns_policy(
	parent_namespace: &str,
	parent_name: &str,
) -> NetworkPolicy {
	NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(ALLOW_INGRESS_NAME.to_string()),
			namespace: Some(preview_namespace_name(parent_namespace, parent_name)),
			labels: Some(preview_namespace_labels(parent_namespace, parent_name)),
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
	parent_namespace: &str,
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
					name: format!(
						"{}-acme-key",
						preview_namespace_name(parent_namespace, parent_name)
					),
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
	issuer.metadata.namespace = Some(preview_namespace_name(parent_namespace, parent_name));
	issuer.metadata.labels = Some(preview_namespace_labels(parent_namespace, parent_name));
	issuer
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn namespace_name_appends_preview_suffix() {
		// Arrange & Act
		let name = preview_namespace_name("tenant-a", "my-app");
		let colliding_name = preview_namespace_name("tenant-b", "my-app");

		// Assert
		assert!(name.starts_with("tenant-a-my-app-"));
		assert!(name.ends_with("-preview"));
		assert!(name.len() <= DNS_1123_LABEL_MAX_LENGTH);
		assert_ne!(name, colliding_name);
	}

	#[rstest]
	fn namespace_name_sanitizes_project_name_for_dns_label() {
		// Arrange & Act
		let dotted_name = preview_namespace_name("tenant-a", "my.app-api");
		let dashed_name = preview_namespace_name("tenant-a", "my-app-api");

		// Assert
		assert!(dotted_name.starts_with("tenant-a-my-app-api-"));
		assert_eq!(dotted_name.find('.'), None);
		assert!(dotted_name.ends_with("-preview"));
		assert!(dotted_name.len() <= DNS_1123_LABEL_MAX_LENGTH);
		assert_ne!(dotted_name, dashed_name);
	}

	#[rstest]
	fn legacy_preview_namespace_matches_recovers_parent_namespace() {
		// Arrange
		let namespace = preview_namespace_name("tenant-a", "my-app");

		// Act
		let matches = legacy_preview_namespace_matches(&namespace, "my-app");

		// Assert
		assert!(matches);
	}

	#[rstest]
	fn legacy_preview_namespace_matches_rejects_truncated_identity_prefix() {
		// Arrange
		let namespace = preview_namespace_name(
			"tenant-with-a-very-long-namespace-name-that-truncates-the-preview-prefix",
			"my-app",
		);

		// Act
		let matches = legacy_preview_namespace_matches(&namespace, "my-app");

		// Assert
		assert!(!matches);
	}

	#[rstest]
	fn legacy_preview_namespace_matches_rejects_hash_mismatch() {
		// Arrange
		let namespace = preview_namespace_name("tenant-a", "my-app");
		let suffix = format!("-{PREVIEW_NAMESPACE_SUFFIX}");
		let namespace_without_suffix = namespace.strip_suffix(&suffix).expect("preview suffix");
		let (identity_prefix, hash) = namespace_without_suffix
			.rsplit_once('-')
			.expect("hash segment");
		let replacement = if hash.starts_with('0') { "1" } else { "0" };
		let namespace = format!(
			"{identity_prefix}-{replacement}{}-{PREVIEW_NAMESPACE_SUFFIX}",
			&hash[1..]
		);

		// Act
		let matches = legacy_preview_namespace_matches(&namespace, "my-app");

		// Assert
		assert!(!matches);
	}

	#[rstest]
	fn labels_record_parent_and_managed_by() {
		// Arrange & Act
		let labels = preview_namespace_labels("tenant-a", "my-app");

		// Assert
		assert_eq!(
			labels.get(PARENT_LABEL_KEY).map(String::as_str),
			Some("my-app")
		);
		assert_eq!(
			labels.get(PARENT_NAMESPACE_LABEL_KEY).map(String::as_str),
			Some("tenant-a")
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
	fn owner_labels_record_parent_identity() {
		// Arrange & Act
		let labels = preview_namespace_owner_labels("tenant-a", "my-app", "uid-1");

		// Assert
		assert_eq!(
			labels.get(PARENT_NAMESPACE_LABEL_KEY).map(String::as_str),
			Some("tenant-a")
		);
		assert_eq!(
			labels.get(PARENT_UID_LABEL_KEY).map(String::as_str),
			Some("uid-1")
		);
	}

	#[rstest]
	fn labels_match_only_exact_preview_owner() {
		// Arrange
		let labels = preview_namespace_owner_labels("tenant-a", "my-app", "uid-1");

		// Act & Assert
		assert!(labels_match_preview_owner(
			Some(&labels),
			"tenant-a",
			"my-app",
			"uid-1"
		));
		assert!(!labels_match_preview_owner(
			Some(&labels),
			"tenant-b",
			"my-app",
			"uid-1"
		));
		assert!(!labels_match_preview_owner(
			Some(&labels),
			"tenant-a",
			"my-app",
			"uid-2"
		));
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
		let quota = build_resource_quota("tenant-a", "my-app", Some(&budget));
		let hard = quota.spec.expect("spec").hard.expect("hard limits");

		// Assert
		assert_eq!(hard.get("limits.cpu").map(|q| &q.0), Some(&"2".to_string()));
		assert_eq!(
			hard.get("limits.memory").map(|q| &q.0),
			Some(&"4Gi".to_string())
		);
		// Pod cap is always present.
		assert!(hard.contains_key("pods"));
		assert_eq!(
			quota.metadata.namespace.as_deref(),
			Some(preview_namespace_name("tenant-a", "my-app").as_str())
		);
	}

	#[rstest]
	fn resource_quota_pod_only_when_no_budget() {
		// Act
		let quota = build_resource_quota("tenant-a", "my-app", None);
		let hard = quota.spec.expect("spec").hard.expect("hard limits");

		// Assert
		assert!(hard.contains_key("pods"));
		assert!(!hard.contains_key("limits.cpu"));
		assert!(!hard.contains_key("limits.memory"));
	}

	#[rstest]
	fn allow_ingress_policy_targets_nginx_namespace_and_dns() {
		// Act
		let policy = build_allow_ingress_and_dns_policy("tenant-a", "my-app");

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
			"tenant-a",
			"my-app",
			"https://acme.example/dir",
			"ops@example.com",
			"nginx",
		);

		// Assert
		assert_eq!(issuer.metadata.name.as_deref(), Some(ISSUER_NAME));
		assert_eq!(
			issuer.metadata.namespace.as_deref(),
			Some(preview_namespace_name("tenant-a", "my-app").as_str())
		);
		assert_eq!(issuer.spec.acme.server, "https://acme.example/dir");
		assert_eq!(issuer.spec.acme.solvers[0].http01.ingress.class, "nginx");
	}
}
