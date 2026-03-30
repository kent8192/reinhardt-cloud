//! NetworkPolicy builders for tenant-level network isolation.
//!
//! Generates three policies per isolated `ReinhardtApp`:
//! 1. Default-deny all ingress/egress
//! 2. Allow ingress from ingress controller and same-app pods
//! 3. Allow egress to DNS, managed services, and user-specified CIDRs

use std::collections::BTreeMap;

use k8s_openapi::api::networking::v1::{
	IPBlock, NetworkPolicy, NetworkPolicyEgressRule, NetworkPolicyIngressRule, NetworkPolicyPeer,
	NetworkPolicyPort, NetworkPolicySpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::ReinhardtApp;
use reinhardt_cloud_types::crd::isolation::NetworkIsolationSpec;

use crate::error::Error;
use crate::resources::labels::owner_reference;

/// Builds a default-deny NetworkPolicy for all traffic to/from the app's pods.
pub(crate) fn build_default_deny_policy(app: &ReinhardtApp) -> Result<NetworkPolicy, Error> {
	let name = format!("{}-deny-all", app.name_any());
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;

	Ok(NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(name),
			namespace: Some(namespace),
			owner_references: Some(vec![owner_ref]),
			labels: Some(BTreeMap::from([(
				"app.kubernetes.io/managed-by".to_string(),
				"reinhardt-cloud-operator".to_string(),
			)])),
			..Default::default()
		},
		spec: Some(NetworkPolicySpec {
			pod_selector: Some(LabelSelector {
				match_labels: Some(BTreeMap::from([(
					"app.kubernetes.io/name".to_string(),
					app.name_any(),
				)])),
				..Default::default()
			}),
			policy_types: Some(vec!["Ingress".to_string(), "Egress".to_string()]),
			ingress: Some(vec![]),
			egress: Some(vec![]),
		}),
	})
}

/// Builds an ingress policy allowing traffic from ingress controllers
/// and same-app pods.
pub(crate) fn build_app_ingress_policy(app: &ReinhardtApp) -> Result<NetworkPolicy, Error> {
	let name = format!("{}-allow-ingress", app.name_any());
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;

	Ok(NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(name),
			namespace: Some(namespace),
			owner_references: Some(vec![owner_ref]),
			labels: Some(BTreeMap::from([(
				"app.kubernetes.io/managed-by".to_string(),
				"reinhardt-cloud-operator".to_string(),
			)])),
			..Default::default()
		},
		spec: Some(NetworkPolicySpec {
			pod_selector: Some(LabelSelector {
				match_labels: Some(BTreeMap::from([(
					"app.kubernetes.io/name".to_string(),
					app.name_any(),
				)])),
				..Default::default()
			}),
			policy_types: Some(vec!["Ingress".to_string()]),
			ingress: Some(vec![
				NetworkPolicyIngressRule {
					from: Some(vec![NetworkPolicyPeer {
						pod_selector: Some(LabelSelector {
							match_labels: Some(BTreeMap::from([(
								"app.kubernetes.io/name".to_string(),
								app.name_any(),
							)])),
							..Default::default()
						}),
						..Default::default()
					}]),
					..Default::default()
				},
				NetworkPolicyIngressRule {
					from: Some(vec![NetworkPolicyPeer {
						namespace_selector: Some(LabelSelector {
							match_labels: Some(BTreeMap::from([(
								"kubernetes.io/metadata.name".to_string(),
								"ingress-nginx".to_string(),
							)])),
							..Default::default()
						}),
						..Default::default()
					}]),
					..Default::default()
				},
			]),
			egress: None,
		}),
	})
}

/// Builds an egress policy allowing DNS, managed services, and user CIDRs.
/// Optionally blocks the cloud metadata service (169.254.169.254).
pub(crate) fn build_managed_service_egress_policy(
	app: &ReinhardtApp,
	network: &NetworkIsolationSpec,
) -> Result<NetworkPolicy, Error> {
	let name = format!("{}-allow-egress", app.name_any());
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;

	let mut egress_rules = vec![NetworkPolicyEgressRule {
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
	}];

	if network.allow_egress {
		let except = if network.block_metadata_service {
			Some(vec!["169.254.169.254/32".to_string()])
		} else {
			None
		};

		egress_rules.push(NetworkPolicyEgressRule {
			to: Some(vec![NetworkPolicyPeer {
				ip_block: Some(IPBlock {
					cidr: "0.0.0.0/0".to_string(),
					except,
				}),
				..Default::default()
			}]),
			..Default::default()
		});
	}

	Ok(NetworkPolicy {
		metadata: ObjectMeta {
			name: Some(name),
			namespace: Some(namespace),
			owner_references: Some(vec![owner_ref]),
			labels: Some(BTreeMap::from([(
				"app.kubernetes.io/managed-by".to_string(),
				"reinhardt-cloud-operator".to_string(),
			)])),
			..Default::default()
		},
		spec: Some(NetworkPolicySpec {
			pod_selector: Some(LabelSelector {
				match_labels: Some(BTreeMap::from([(
					"app.kubernetes.io/name".to_string(),
					app.name_any(),
				)])),
				..Default::default()
			}),
			policy_types: Some(vec!["Egress".to_string()]),
			ingress: None,
			egress: Some(egress_rules),
		}),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ReinhardtAppSpec;
	use rstest::rstest;

	fn test_app() -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some("myapp".to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "test:latest".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn default_deny_policy_has_correct_selector() {
		// Arrange
		let app = test_app();

		// Act
		let policy = build_default_deny_policy(&app).unwrap();

		// Assert
		let spec = policy.spec.unwrap();
		let pod_selector = spec.pod_selector.unwrap();
		let selector = pod_selector.match_labels.unwrap();
		assert_eq!(selector.get("app.kubernetes.io/name").unwrap(), "myapp");
		assert_eq!(spec.policy_types.as_ref().unwrap().len(), 2);
	}

	#[rstest]
	fn default_deny_policy_has_owner_reference() {
		// Arrange
		let app = test_app();

		// Act
		let policy = build_default_deny_policy(&app).unwrap();

		// Assert
		let owner_refs = policy.metadata.owner_references.unwrap();
		assert_eq!(owner_refs.len(), 1);
		assert_eq!(owner_refs[0].name, "myapp");
	}

	#[rstest]
	fn egress_policy_blocks_imds_by_default() {
		// Arrange
		let app = test_app();
		let network = NetworkIsolationSpec::default();

		// Act
		let policy = build_managed_service_egress_policy(&app, &network).unwrap();

		// Assert
		let spec = policy.spec.unwrap();
		let egress_rules = spec.egress.unwrap();
		assert!(egress_rules.len() >= 2);
		let has_imds_block = egress_rules.iter().any(|r| {
			r.to.as_ref()
				.map(|peers| {
					peers.iter().any(|p| {
						p.ip_block
							.as_ref()
							.map(|b| {
								b.except
									.as_ref()
									.map(|e| e.contains(&"169.254.169.254/32".to_string()))
									.unwrap_or(false)
							})
							.unwrap_or(false)
					})
				})
				.unwrap_or(false)
		});
		assert!(has_imds_block);
	}

	#[rstest]
	fn egress_policy_allows_imds_when_disabled() {
		// Arrange
		let app = test_app();
		let network = NetworkIsolationSpec {
			block_metadata_service: false,
			..Default::default()
		};

		// Act
		let policy = build_managed_service_egress_policy(&app, &network).unwrap();

		// Assert
		let spec = policy.spec.unwrap();
		let egress_rules = spec.egress.unwrap();
		let has_imds_block = egress_rules.iter().any(|r| {
			r.to.as_ref()
				.map(|peers| {
					peers.iter().any(|p| {
						p.ip_block
							.as_ref()
							.map(|b| {
								b.except
									.as_ref()
									.map(|e| e.contains(&"169.254.169.254/32".to_string()))
									.unwrap_or(false)
							})
							.unwrap_or(false)
					})
				})
				.unwrap_or(false)
		});
		assert!(!has_imds_block);
	}

	#[rstest]
	fn egress_policy_always_allows_dns() {
		// Arrange
		let app = test_app();
		let network = NetworkIsolationSpec::default();

		// Act
		let policy = build_managed_service_egress_policy(&app, &network).unwrap();

		// Assert
		let spec = policy.spec.unwrap();
		let egress_rules = spec.egress.unwrap();
		let dns_rule = egress_rules.iter().find(|r| {
			r.ports
				.as_ref()
				.map(|ports| ports.iter().any(|p| p.port == Some(IntOrString::Int(53))))
				.unwrap_or(false)
		});
		assert!(dns_rule.is_some());
	}
}
