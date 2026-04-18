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
	let crd = crds.get(CRD_NAME).await.map_err(|e| {
		std::io::Error::other(format!("failed to fetch CRD {CRD_NAME} from cluster: {e}"))
	})?;

	let chosen = pick_best_version(&crd)
		.ok_or_else(|| std::io::Error::other(format!("CRD {CRD_NAME} has no served versions")))?;

	let resolved = format!("{CRD_GROUP}/{chosen}");

	let served: Vec<&str> = crd
		.spec
		.versions
		.iter()
		.filter(|v| v.served)
		.map(|v| v.name.as_str())
		.collect();

	// Strip the group prefix without allocating on every call. `split_once`
	// returns `None` for malformed inputs, in which case we fall back to the
	// raw string so the warning still surfaces a meaningful comparison.
	let compile_default_version = COMPILE_TIME_DEFAULT
		.split_once('/')
		.map(|(_, version)| version)
		.unwrap_or(COMPILE_TIME_DEFAULT);
	if !served.contains(&compile_default_version) {
		eprintln!(
			"Warning: compiled-in default {COMPILE_TIME_DEFAULT} is not served by the cluster; using {resolved} instead"
		);
	}

	Ok(resolved)
}

/// Selects the best served version name from a CRD spec.
///
/// Selection rules:
/// 1. If a version is both `served: true` and `storage: true`, return it.
/// 2. Otherwise, return the highest-ranking version by `version_rank`.
/// 3. Returns `None` when no served versions exist.
///
/// This is the shared core used by both production code
/// (`resolve_api_version`) and tests, so they cannot drift apart.
pub(crate) fn pick_best_version(crd: &CustomResourceDefinition) -> Option<String> {
	let served: Vec<&str> = crd
		.spec
		.versions
		.iter()
		.filter(|v| v.served)
		.map(|v| v.name.as_str())
		.collect();

	if served.is_empty() {
		return None;
	}

	if let Some(storage) = crd
		.spec
		.versions
		.iter()
		.find(|v| v.served && v.storage)
		.map(|v| v.name.as_str())
	{
		return Some(storage.to_string());
	}

	// Use max_by_key to avoid re-parsing on every comparison; compare the
	// &str directly as a tie-breaker to avoid allocating a String for every
	// candidate when two versions share the same numeric rank.
	served
		.into_iter()
		.max_by_key(|v| (version_rank(v), *v))
		.map(str::to_string)
}

/// Assigns a numeric rank to a Kubernetes API version string.
///
/// Higher ranks represent more stable versions. Stability tier takes
/// priority over major version number, so `v1` outranks `v2alpha1`.
/// Ordering: `v<N>` > `v<N>beta<M>` > `v<N>alpha<M>`. Unknown formats
/// get the lowest rank so they do not accidentally win over recognized
/// versions.
///
/// Rank layout (tier + major*1000 + minor):
/// - Stable  (no suffix): 2_000_000 + major*1000 + 0
/// - Beta   (vNbetaM):    1_000_000 + major*1000 + minor
/// - Alpha  (vNalphaM):   0         + major*1000 + minor
/// - Unknown:             0 (raw, no major multiplier)
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

	if suffix.is_empty() {
		// Stable: vN — tier 2_000_000
		2_000_000 + major * 1_000
	} else if let Some(minor_str) = suffix.strip_prefix("beta") {
		let minor = minor_str.parse::<u32>().unwrap_or(0);
		// Beta: vNbetaM — tier 1_000_000
		1_000_000 + major * 1_000 + minor
	} else if let Some(minor_str) = suffix.strip_prefix("alpha") {
		let minor = minor_str.parse::<u32>().unwrap_or(0);
		// Alpha: vNalphaM — tier 0
		major * 1_000 + minor
	} else {
		// Unknown suffix (e.g., `v2foo`): return 0 so non-standard
		// versions never outrank recognized ones, even across major versions.
		0
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
	// Stability tier must beat a higher major number:
	// v1 (stable) must outrank v2alpha1 even though 2 > 1.
	#[case("v1", "v2alpha1")]
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

	#[rstest]
	#[case("v2foo", "v1")]
	#[case("v2foo", "v1alpha1")]
	#[case("v99bogus", "v1")]
	fn test_version_rank_unknown_suffix_ranks_below_known(
		#[case] unknown: &str,
		#[case] known: &str,
	) {
		// Act
		let u = version_rank(unknown);
		let k = version_rank(known);

		// Assert: non-standard suffixes must never outrank recognized versions,
		// even when their major number is higher.
		assert!(u < k, "expected {unknown} < {known}, got {u} vs {k}");
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

	/// Test wrapper around the production `pick_best_version` function.
	///
	/// Tests exercise the exact same selection logic used by
	/// `resolve_api_version`, so they cannot drift from production code.
	fn resolve_from_crd(crd: &CustomResourceDefinition) -> Result<String, String> {
		let chosen = pick_best_version(crd).ok_or_else(|| "no served versions".to_string())?;
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
