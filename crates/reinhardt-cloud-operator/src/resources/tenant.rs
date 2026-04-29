//! Builders for tenant-scoped Kubernetes primitives (#416).
//!
//! When a `ReinhardtApp` sets `spec.tenant`, the operator reconciles a
//! deterministic per-tenant `Namespace`, `ResourceQuota`, and a
//! default-deny + selective-allow `NetworkPolicy` triple in addition to
//! the per-app resources defined in the sibling modules. The builders
//! in this module are pure functions that produce the desired Kubernetes
//! resource — they do not talk to the API server.
//!
//! Tenant resources do not carry an `ownerReference` to the
//! `ReinhardtApp` because a single namespace is shared by every app in
//! the tenant; tying its lifetime to one specific CR would cause
//! cascade-deletion when that CR is removed even though sibling CRs
//! still need the namespace. Cleanup of an empty tenant namespace is
//! intentionally left out of scope for the per-app reconciler and
//! handled by a future tenant-scoped controller.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Namespace, ResourceQuota, ResourceQuotaSpec};
use k8s_openapi::api::networking::v1::{
	NetworkPolicy, NetworkPolicyEgressRule, NetworkPolicyIngressRule, NetworkPolicyPeer,
	NetworkPolicyPort, NetworkPolicySpec,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use reinhardt_cloud_types::crd::tenant::TenantRef;

/// Label key that records the owning tenant on every resource the
/// operator generates for a tenant. Pairs with the `kubernetes.io/metadata.name`
/// label that Kubernetes itself sets on namespaces, but is more specific
/// (selects only operator-owned namespaces).
pub(crate) const TENANT_LABEL_KEY: &str = "paas.reinhardt-cloud.dev/tenant";

/// Optional label naming the team within a tenant. Set only when
/// `TenantRef.team` is present.
pub(crate) const TENANT_TEAM_LABEL_KEY: &str = "paas.reinhardt-cloud.dev/team";

/// Standard managed-by label value used by every operator-owned
/// resource so cleanup tooling can identify operator-managed objects.
const MANAGED_BY_VALUE: &str = "reinhardt-cloud-operator";

/// Conventional name suffix for the tenant-default ResourceQuota.
const DEFAULT_QUOTA_NAME: &str = "tenant-default-quota";

/// Conventional names for the tenant-default NetworkPolicy triple.
const DEFAULT_DENY_POLICY_NAME: &str = "tenant-default-deny";
const ALLOW_SAME_NAMESPACE_POLICY_NAME: &str = "tenant-allow-same-namespace";
const ALLOW_INGRESS_CONTROLLER_POLICY_NAME: &str = "tenant-allow-ingress-controller";

/// Namespace selector label commonly applied by the NGINX ingress
/// controller chart. Used to allow ingress traffic from the controller
/// into the tenant namespace.
const INGRESS_CONTROLLER_NAMESPACE_LABEL: &str = "ingress-nginx";

/// Operator-default `ResourceQuota` values. Override per CR is tracked
/// in #416's follow-up work; for now the same defaults apply to every
/// tenant.
const DEFAULT_QUOTA_REQUESTS_CPU: &str = "10";
const DEFAULT_QUOTA_REQUESTS_MEMORY: &str = "20Gi";
const DEFAULT_QUOTA_LIMITS_CPU: &str = "20";
const DEFAULT_QUOTA_LIMITS_MEMORY: &str = "40Gi";
const DEFAULT_QUOTA_PVC_COUNT: &str = "20";
const DEFAULT_QUOTA_PVC_STORAGE: &str = "200Gi";
const DEFAULT_QUOTA_POD_COUNT: &str = "100";

/// Build the standard set of labels applied to every operator-managed
/// tenant-scoped resource (namespace, quota, network policy).
pub(crate) fn tenant_labels(tenant: &TenantRef) -> BTreeMap<String, String> {
	let mut labels = BTreeMap::from([
		(
			"app.kubernetes.io/managed-by".to_string(),
			MANAGED_BY_VALUE.to_string(),
		),
		(TENANT_LABEL_KEY.to_string(), tenant.organization.clone()),
	]);
	if let Some(team) = &tenant.team {
		labels.insert(TENANT_TEAM_LABEL_KEY.to_string(), team.clone());
	}
	labels
}

/// Builds the `Namespace` resource that owns every workload belonging
/// to this tenant.
///
/// Pod Security Standards labels are intentionally NOT set here so the
/// existing per-app `reconcile_pss_labels` path remains the single
/// writer for that group of labels. Future work may consolidate PSS
/// labeling into this module.
pub(crate) fn build_namespace(tenant: &TenantRef) -> Namespace {
	Namespace {
		metadata: ObjectMeta {
			name: Some(tenant.namespace()),
			labels: Some(tenant_labels(tenant)),
			..Default::default()
		},
		..Default::default()
	}
}

/// Builds the operator-default `ResourceQuota` for a tenant namespace.
///
/// The quota caps aggregate compute, storage, and pod-count usage so a
/// runaway workload from one tenant cannot starve the cluster.
pub(crate) fn build_default_resource_quota(tenant: &TenantRef) -> ResourceQuota {
	let hard = BTreeMap::from([
		(
			"requests.cpu".to_string(),
			Quantity(DEFAULT_QUOTA_REQUESTS_CPU.to_string()),
		),
		(
			"requests.memory".to_string(),
			Quantity(DEFAULT_QUOTA_REQUESTS_MEMORY.to_string()),
		),
		(
			"limits.cpu".to_string(),
			Quantity(DEFAULT_QUOTA_LIMITS_CPU.to_string()),
		),
		(
			"limits.memory".to_string(),
			Quantity(DEFAULT_QUOTA_LIMITS_MEMORY.to_string()),
		),
		(
			"persistentvolumeclaims".to_string(),
			Quantity(DEFAULT_QUOTA_PVC_COUNT.to_string()),
		),
		(
			"requests.storage".to_string(),
			Quantity(DEFAULT_QUOTA_PVC_STORAGE.to_string()),
		),
		(
			"pods".to_string(),
			Quantity(DEFAULT_QUOTA_POD_COUNT.to_string()),
		),
	]);

	ResourceQuota {
		metadata: ObjectMeta {
			name: Some(DEFAULT_QUOTA_NAME.to_string()),
			namespace: Some(tenant.namespace()),
			labels: Some(tenant_labels(tenant)),
			..Default::default()
		},
		spec: Some(ResourceQuotaSpec {
			hard: Some(hard),
			..Default::default()
		}),
		..Default::default()
	}
}

/// Builds a tenant-wide default-deny `NetworkPolicy`.
///
/// Selects every Pod in the tenant namespace (empty `pod_selector`) and
/// declares both `Ingress` and `Egress` policy types with no rules,
/// which Kubernetes interprets as "deny all" for the selected pods.
/// Subsequent allow-policies layer on top of this baseline.
pub(crate) fn build_default_deny_policy(tenant: &TenantRef) -> NetworkPolicy {
	NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(DEFAULT_DENY_POLICY_NAME.to_string()),
			namespace: Some(tenant.namespace()),
			labels: Some(tenant_labels(tenant)),
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

/// Builds a `NetworkPolicy` that allows traffic between Pods within
/// the tenant namespace and DNS egress to the cluster's kube-dns. This
/// is the minimum surface needed for an app's web tier to reach its
/// own database/cache without re-enabling cross-tenant traffic.
pub(crate) fn build_allow_same_namespace_policy(tenant: &TenantRef) -> NetworkPolicy {
	let same_namespace_peer = NetworkPolicyPeer {
		pod_selector: Some(LabelSelector::default()),
		..Default::default()
	};

	NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(ALLOW_SAME_NAMESPACE_POLICY_NAME.to_string()),
			namespace: Some(tenant.namespace()),
			labels: Some(tenant_labels(tenant)),
			..Default::default()
		},
		spec: Some(NetworkPolicySpec {
			pod_selector: Some(LabelSelector::default()),
			policy_types: Some(vec!["Ingress".to_string(), "Egress".to_string()]),
			ingress: Some(vec![NetworkPolicyIngressRule {
				from: Some(vec![same_namespace_peer.clone()]),
				..Default::default()
			}]),
			egress: Some(vec![
				// Same-namespace egress (mirrors the ingress rule).
				NetworkPolicyEgressRule {
					to: Some(vec![same_namespace_peer]),
					..Default::default()
				},
				// DNS — required for service discovery. Permitted to
				// any destination because kube-dns selectors vary
				// across distros (kube-system / openshift-dns / etc.).
				NetworkPolicyEgressRule {
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
				},
			]),
		}),
	}
}

/// Builds a `NetworkPolicy` allowing the cluster's NGINX ingress
/// controller to reach Pods in the tenant namespace.
///
/// Selects pods only by namespace (the canonical `ingress-nginx`
/// namespace label set by the chart). Distros that ship a different
/// ingress controller will need to override this policy in a follow-up
/// PR — the issue spec explicitly singles out the ingress controller
/// as the one external system that MUST be allowed in.
pub(crate) fn build_allow_ingress_controller_policy(tenant: &TenantRef) -> NetworkPolicy {
	NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(ALLOW_INGRESS_CONTROLLER_POLICY_NAME.to_string()),
			namespace: Some(tenant.namespace()),
			labels: Some(tenant_labels(tenant)),
			..Default::default()
		},
		spec: Some(NetworkPolicySpec {
			pod_selector: Some(LabelSelector::default()),
			policy_types: Some(vec!["Ingress".to_string()]),
			ingress: Some(vec![NetworkPolicyIngressRule {
				from: Some(vec![NetworkPolicyPeer {
					namespace_selector: Some(LabelSelector {
						match_labels: Some(BTreeMap::from([(
							"kubernetes.io/metadata.name".to_string(),
							INGRESS_CONTROLLER_NAMESPACE_LABEL.to_string(),
						)])),
						..Default::default()
					}),
					..Default::default()
				}]),
				..Default::default()
			}]),
			egress: None,
		}),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn tenant_org() -> TenantRef {
		TenantRef {
			organization: "acme".to_string(),
			team: None,
		}
	}

	fn tenant_org_team() -> TenantRef {
		TenantRef {
			organization: "acme".to_string(),
			team: Some("platform".to_string()),
		}
	}

	#[rstest]
	fn tenant_labels_includes_organization_and_managed_by() {
		// Arrange
		let tenant = tenant_org();

		// Act
		let labels = tenant_labels(&tenant);

		// Assert
		assert_eq!(
			labels.get(TENANT_LABEL_KEY).map(String::as_str),
			Some("acme")
		);
		assert_eq!(
			labels
				.get("app.kubernetes.io/managed-by")
				.map(String::as_str),
			Some(MANAGED_BY_VALUE),
		);
		assert!(labels.get(TENANT_TEAM_LABEL_KEY).is_none());
	}

	#[rstest]
	fn tenant_labels_includes_team_when_present() {
		// Arrange
		let tenant = tenant_org_team();

		// Act
		let labels = tenant_labels(&tenant);

		// Assert
		assert_eq!(
			labels.get(TENANT_TEAM_LABEL_KEY).map(String::as_str),
			Some("platform"),
		);
	}

	#[rstest]
	fn build_namespace_uses_computed_name() {
		// Arrange
		let tenant = tenant_org_team();

		// Act
		let namespace = build_namespace(&tenant);

		// Assert
		assert_eq!(
			namespace.metadata.name.as_deref(),
			Some("tenant-acme-platform"),
		);
		// Tenant labels MUST be present so cleanup tooling can find the
		// namespace by selector.
		let labels = namespace.metadata.labels.expect("labels");
		assert_eq!(
			labels.get(TENANT_LABEL_KEY).map(String::as_str),
			Some("acme")
		);
	}

	#[rstest]
	fn build_default_resource_quota_targets_tenant_namespace() {
		// Arrange
		let tenant = tenant_org();

		// Act
		let quota = build_default_resource_quota(&tenant);

		// Assert
		assert_eq!(quota.metadata.namespace.as_deref(), Some("tenant-acme"),);
		assert_eq!(quota.metadata.name.as_deref(), Some(DEFAULT_QUOTA_NAME),);
		let hard = quota.spec.expect("spec").hard.expect("hard limits");
		// Sanity-check that all the documented hard limits are present.
		assert!(hard.contains_key("requests.cpu"));
		assert!(hard.contains_key("limits.memory"));
		assert!(hard.contains_key("pods"));
		assert!(hard.contains_key("persistentvolumeclaims"));
	}

	#[rstest]
	fn default_deny_policy_selects_all_pods() {
		// Arrange
		let tenant = tenant_org();

		// Act
		let policy = build_default_deny_policy(&tenant);

		// Assert
		let spec = policy.spec.expect("spec");
		// An empty pod_selector matches every Pod in the namespace,
		// which is the canonical default-deny shape.
		let pod_selector = spec.pod_selector.expect("pod_selector");
		assert!(pod_selector.match_labels.is_none());
		assert!(pod_selector.match_expressions.is_none());
		// Both policy types declared with empty rule arrays = deny.
		let policy_types = spec.policy_types.expect("policy_types");
		assert!(policy_types.contains(&"Ingress".to_string()));
		assert!(policy_types.contains(&"Egress".to_string()));
		assert!(spec.ingress.expect("ingress").is_empty());
		assert!(spec.egress.expect("egress").is_empty());
	}

	#[rstest]
	fn allow_same_namespace_policy_includes_dns_egress() {
		// Arrange
		let tenant = tenant_org();

		// Act
		let policy = build_allow_same_namespace_policy(&tenant);

		// Assert — there MUST be a DNS rule on port 53 so service
		// discovery works inside the tenant namespace even with the
		// default-deny policy in place.
		let egress = policy.spec.expect("spec").egress.expect("egress");
		let has_dns = egress.iter().any(|rule| {
			rule.ports
				.as_ref()
				.map(|ports| ports.iter().any(|p| p.port == Some(IntOrString::Int(53))))
				.unwrap_or(false)
		});
		assert!(has_dns, "expected DNS egress rule, got {egress:?}");
	}

	#[rstest]
	fn allow_same_namespace_policy_permits_in_namespace_traffic() {
		// Arrange
		let tenant = tenant_org();

		// Act
		let policy = build_allow_same_namespace_policy(&tenant);

		// Assert — the ingress rule MUST allow same-namespace pods
		// (empty pod_selector) so the web tier can talk to the worker
		// tier inside the tenant.
		let spec = policy.spec.expect("spec");
		let ingress = spec.ingress.expect("ingress");
		assert_eq!(ingress.len(), 1);
		let from = ingress[0].from.as_ref().expect("from");
		assert!(
			from.iter().any(|peer| peer.pod_selector.is_some()),
			"expected same-namespace pod selector peer",
		);
	}

	#[rstest]
	fn allow_ingress_controller_policy_targets_nginx_namespace() {
		// Arrange
		let tenant = tenant_org();

		// Act
		let policy = build_allow_ingress_controller_policy(&tenant);

		// Assert — the namespace selector must match the canonical
		// `ingress-nginx` namespace, otherwise the cluster ingress
		// path is silently broken.
		let spec = policy.spec.expect("spec");
		let from = spec.ingress.expect("ingress")[0]
			.from
			.clone()
			.expect("from");
		let ns_selector = from[0]
			.namespace_selector
			.as_ref()
			.expect("namespace_selector");
		let labels = ns_selector.match_labels.as_ref().expect("match_labels");
		assert_eq!(
			labels
				.get("kubernetes.io/metadata.name")
				.map(String::as_str),
			Some(INGRESS_CONTROLLER_NAMESPACE_LABEL),
		);
	}

	#[rstest]
	fn tenant_resources_share_consistent_labels() {
		// Arrange
		let tenant = tenant_org_team();

		// Act
		let namespace = build_namespace(&tenant);
		let quota = build_default_resource_quota(&tenant);
		let deny = build_default_deny_policy(&tenant);
		let allow_same = build_allow_same_namespace_policy(&tenant);
		let allow_ingress = build_allow_ingress_controller_policy(&tenant);

		// Assert — every operator-managed resource MUST be discoverable
		// by the same `paas.reinhardt-cloud.dev/tenant=<org>` label so
		// cleanup tooling can use a single selector.
		for labels in [
			namespace.metadata.labels.as_ref(),
			quota.metadata.labels.as_ref(),
			deny.metadata.labels.as_ref(),
			allow_same.metadata.labels.as_ref(),
			allow_ingress.metadata.labels.as_ref(),
		] {
			let labels = labels.expect("labels");
			assert_eq!(
				labels.get(TENANT_LABEL_KEY).map(String::as_str),
				Some("acme"),
			);
			assert_eq!(
				labels.get(TENANT_TEAM_LABEL_KEY).map(String::as_str),
				Some("platform"),
			);
		}
	}
}
