//! Tests for the `oauth_providers` discovery endpoint helpers.
//!
//! The discovery endpoint MUST NOT leak provider secrets, redirect URIs,
//! or client IDs into the response body — only the public-facing `id` and
//! human-readable `label` per provider. These tests pin that contract on
//! the response struct itself so a future field added to `ProviderEntry`
//! cannot silently start exposing secrets.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use serde_json::Value;

	use crate::apps::auth::views::oauth::providers::{ProviderEntry, ProvidersResponse, label_for};

	#[rstest]
	fn test_provider_entry_serializes_only_id_and_label() {
		// Arrange
		let entry = ProviderEntry {
			id: "github",
			label: "GitHub",
		};

		// Act
		let json = serde_json::to_value(&entry).expect("serialize entry");

		// Assert — the object has exactly the two public fields and nothing
		// else. If a future change adds a credential-bearing field, this
		// assertion fails.
		let obj = json.as_object().expect("entry is object");
		assert_eq!(
			obj.len(),
			2,
			"ProviderEntry must serialize to exactly 2 fields, got {}: {json:?}",
			obj.len()
		);
		assert_eq!(obj.get("id").and_then(Value::as_str), Some("github"));
		assert_eq!(obj.get("label").and_then(Value::as_str), Some("GitHub"));
	}

	#[rstest]
	fn test_providers_response_does_not_contain_secret_keywords() {
		// Arrange — populate with one entry to mirror the runtime shape.
		let resp = ProvidersResponse {
			providers: vec![ProviderEntry {
				id: "github",
				label: "GitHub",
			}],
		};

		// Act
		let json = serde_json::to_string(&resp).expect("serialize response");
		let lowered = json.to_lowercase();

		// Assert — none of the well-known secret tokens appear anywhere in
		// the response body.
		for forbidden in [
			"client_id",
			"client_secret",
			"redirect_uri",
			"secret",
			"token",
			"password",
		] {
			assert!(
				!lowered.contains(forbidden),
				"providers response leaked '{forbidden}': {json}"
			);
		}
	}

	#[rstest]
	#[case("github", "GitHub")]
	#[case("unknown", "OAuth")]
	#[case("", "OAuth")]
	fn test_label_for_maps_known_ids(#[case] id: &str, #[case] expected: &str) {
		// Act / Assert
		assert_eq!(label_for(id), expected);
	}

	#[rstest]
	fn test_empty_providers_response_serializes_to_empty_array() {
		// Arrange
		let resp = ProvidersResponse { providers: vec![] };

		// Act
		let json = serde_json::to_value(&resp).expect("serialize empty");

		// Assert
		assert_eq!(json["providers"], Value::Array(vec![]));
	}
}
