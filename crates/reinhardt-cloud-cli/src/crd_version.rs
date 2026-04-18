//! Runtime discovery of the served `ReinhardtApp` CRD apiVersion.
//!
//! The CLI is compiled against a single default apiVersion
//! (`COMPILE_TIME_DEFAULT`), but the target cluster may serve a different
//! version. `resolve_api_version` queries the cluster's
//! `CustomResourceDefinition` for `reinhardtapps.paas.reinhardt-cloud.dev`
//! and chooses the best available served version.
//!
//! Selection priority (among `served: true` entries):
//! 1. The version marked `storage: true`.
//! 2. Otherwise, the highest-ranking version by descending order:
//!    `v1 > v1beta1 > v1alpha2 > v1alpha1`.
//!
//! If the user passes an explicit override, the override is returned as-is
//! without contacting the cluster.

use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{Api, Client};

/// Fully qualified apiVersion the CLI is compiled against.
pub(crate) const COMPILE_TIME_DEFAULT: &str = "paas.reinhardt-cloud.dev/v1alpha2";

/// Group portion of the `ReinhardtApp` CRD (without the version suffix).
const CRD_GROUP: &str = "paas.reinhardt-cloud.dev";

/// Name of the `ReinhardtApp` CRD in the cluster.
const CRD_NAME: &str = "reinhardtapps.paas.reinhardt-cloud.dev";

/// Resolves the apiVersion to use when applying `ReinhardtApp` resources.
///
/// - When `override_version` is `Some`, returns it verbatim without any
///   cluster interaction.
/// - Otherwise, inspects the live CRD and picks the preferred served
///   version per the rules documented on this module.
/// - If the compile-time default is not among the served versions, a
///   warning is printed to stderr and the resolved version is returned
///   anyway.
pub(crate) async fn resolve_api_version(
	client: &Client,
	override_version: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
	if let Some(explicit) = override_version {
		return Ok(explicit.to_string());
	}

	let crds: Api<CustomResourceDefinition> = Api::all(client.clone());
	let crd = crds
		.get(CRD_NAME)
		.await
		.map_err(|e| format!("failed to fetch CRD {CRD_NAME} from cluster: {e}"))?;

	let served: Vec<&str> = crd
		.spec
		.versions
		.iter()
		.filter(|v| v.served)
		.map(|v| v.name.as_str())
		.collect();

	if served.is_empty() {
		return Err(format!("CRD {CRD_NAME} has no served versions").into());
	}

	// Prefer the storage version when available.
	let storage_version = crd
		.spec
		.versions
		.iter()
		.find(|v| v.served && v.storage)
		.map(|v| v.name.as_str());

	let chosen = if let Some(storage) = storage_version {
		storage.to_string()
	} else {
		let mut candidates: Vec<&str> = served.clone();
		candidates.sort_by(|a, b| version_rank(b).cmp(&version_rank(a)));
		candidates[0].to_string()
	};

	let resolved = format!("{CRD_GROUP}/{chosen}");

	let compile_default_version = COMPILE_TIME_DEFAULT
		.strip_prefix(&format!("{CRD_GROUP}/"))
		.unwrap_or(COMPILE_TIME_DEFAULT);
	if !served.iter().any(|v| *v == compile_default_version) {
		eprintln!(
			"Warning: compiled-in default {COMPILE_TIME_DEFAULT} is not served by the cluster; using {resolved} instead"
		);
	}

	Ok(resolved)
}

/// Assigns a numeric rank to a Kubernetes API version string.
///
/// Higher ranks represent more stable versions. Ordering:
/// `v<N>` > `v<N>beta<M>` > `v<N>alpha<M>`. Unknown formats get the
/// lowest rank so they do not accidentally win over recognized versions.
fn version_rank(version: &str) -> u32 {
	// Strip leading 'v'.
	let Some(rest) = version.strip_prefix('v') else {
		return 0;
	};

	// Find where the major number ends.
	let major_end = rest
		.find(|c: char| !c.is_ascii_digit())
		.unwrap_or(rest.len());
	let Ok(major) = rest[..major_end].parse::<u32>() else {
		return 0;
	};
	let suffix = &rest[major_end..];

	// Base rank: stable versions get the highest band.
	let base = major * 1_000_000;

	if suffix.is_empty() {
		// Stable: vN
		base + 900_000
	} else if let Some(minor) = suffix.strip_prefix("beta") {
		let m = minor.parse::<u32>().unwrap_or(0);
		base + 500_000 + m
	} else if let Some(minor) = suffix.strip_prefix("alpha") {
		let m = minor.parse::<u32>().unwrap_or(0);
		base + 100_000 + m
	} else {
		base
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case("v1", "v1beta1")]
	#[case("v1", "v1alpha2")]
	#[case("v1beta1", "v1alpha2")]
	#[case("v1alpha2", "v1alpha1")]
	#[case("v2", "v1")]
	#[case("v1beta2", "v1beta1")]
	fn test_version_rank_orders_stable_above_pre_release(
		#[case] higher: &str,
		#[case] lower: &str,
	) {
		// Act
		let h = version_rank(higher);
		let l = version_rank(lower);

		// Assert
		assert!(h > l, "expected {higher} > {lower}, got {h} vs {l}");
	}

	#[rstest]
	fn test_version_rank_unknown_is_lowest() {
		// Arrange / Act
		let unknown = version_rank("banana");
		let v1alpha1 = version_rank("v1alpha1");

		// Assert
		assert!(unknown < v1alpha1);
	}

	/// Builds a `CustomResourceDefinition` from a JSON spec for tests.
	fn make_crd(versions: serde_json::Value) -> CustomResourceDefinition {
		let doc = serde_json::json!({
			"apiVersion": "apiextensions.k8s.io/v1",
			"kind": "CustomResourceDefinition",
			"metadata": { "name": CRD_NAME },
			"spec": {
				"group": CRD_GROUP,
				"names": {
					"plural": "reinhardtapps",
					"singular": "reinhardtapp",
					"kind": "ReinhardtApp",
					"listKind": "ReinhardtAppList",
				},
				"scope": "Namespaced",
				"versions": versions,
			},
		});
		serde_json::from_value(doc).expect("valid CRD fixture")
	}

	/// Helper: run the non-client-hitting portions of `resolve_api_version`
	/// against a synthetic CRD, mirroring the logic under test.
	fn resolve_from_crd(crd: &CustomResourceDefinition) -> Result<String, String> {
		let served: Vec<&str> = crd
			.spec
			.versions
			.iter()
			.filter(|v| v.served)
			.map(|v| v.name.as_str())
			.collect();

		if served.is_empty() {
			return Err("no served versions".to_string());
		}

		let storage_version = crd
			.spec
			.versions
			.iter()
			.find(|v| v.served && v.storage)
			.map(|v| v.name.as_str());

		let chosen = if let Some(storage) = storage_version {
			storage.to_string()
		} else {
			let mut candidates: Vec<&str> = served.clone();
			candidates.sort_by(|a, b| version_rank(b).cmp(&version_rank(a)));
			candidates[0].to_string()
		};

		Ok(format!("{CRD_GROUP}/{chosen}"))
	}

	#[rstest]
	fn test_resolve_prefers_storage_version_over_rank() {
		// Arrange: v1alpha2 is storage, v1 is served but not storage.
		let crd = make_crd(serde_json::json!([
			{ "name": "v1", "served": true, "storage": false, "schema": { "openAPIV3Schema": { "type": "object" } } },
			{ "name": "v1alpha2", "served": true, "storage": true, "schema": { "openAPIV3Schema": { "type": "object" } } },
		]));

		// Act
		let resolved = resolve_from_crd(&crd).unwrap();

		// Assert
		assert_eq!(resolved, "paas.reinhardt-cloud.dev/v1alpha2");
	}

	#[rstest]
	fn test_resolve_falls_back_to_highest_served_without_storage() {
		// Arrange: no served version has storage=true; highest served should win.
		let crd = make_crd(serde_json::json!([
			{ "name": "v1alpha1", "served": true, "storage": false, "schema": { "openAPIV3Schema": { "type": "object" } } },
			{ "name": "v1beta1", "served": true, "storage": false, "schema": { "openAPIV3Schema": { "type": "object" } } },
			{ "name": "v1alpha2", "served": true, "storage": false, "schema": { "openAPIV3Schema": { "type": "object" } } },
		]));

		// Act
		let resolved = resolve_from_crd(&crd).unwrap();

		// Assert
		assert_eq!(resolved, "paas.reinhardt-cloud.dev/v1beta1");
	}

	#[rstest]
	fn test_resolve_skips_unserved_versions() {
		// Arrange: v1 exists but is not served.
		let crd = make_crd(serde_json::json!([
			{ "name": "v1", "served": false, "storage": false, "schema": { "openAPIV3Schema": { "type": "object" } } },
			{ "name": "v1alpha2", "served": true, "storage": true, "schema": { "openAPIV3Schema": { "type": "object" } } },
		]));

		// Act
		let resolved = resolve_from_crd(&crd).unwrap();

		// Assert
		assert_eq!(resolved, "paas.reinhardt-cloud.dev/v1alpha2");
	}

	#[rstest]
	fn test_resolve_errors_when_no_served_versions() {
		// Arrange
		let crd = make_crd(serde_json::json!([
			{ "name": "v1alpha2", "served": false, "storage": true, "schema": { "openAPIV3Schema": { "type": "object" } } },
		]));

		// Act
		let result = resolve_from_crd(&crd);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	#[tokio::test]
	async fn test_override_short_circuits_without_client_call() {
		// Arrange: build a Client pointed at an unreachable URL; if the
		// override path accidentally contacts it, the test will fail.
		let config = kube::Config::new("http://127.0.0.1:1".parse().unwrap());
		let client = Client::try_from(config).expect("build client");

		// Act
		let resolved = resolve_api_version(&client, Some("paas.reinhardt-cloud.dev/v9"))
			.await
			.expect("override must not require cluster access");

		// Assert
		assert_eq!(resolved, "paas.reinhardt-cloud.dev/v9");
	}
}
