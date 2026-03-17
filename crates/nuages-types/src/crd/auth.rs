use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Authentication specification
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthSpec {
	/// JWT authentication enabled
	#[serde(default)]
	pub jwt: bool,
	/// OAuth2 configuration
	pub oauth: Option<OAuthSpec>,
}

/// OAuth2 provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OAuthSpec {
	/// OAuth provider name
	pub provider: String,
	/// Secret name containing client_id and client_secret
	pub credentials_secret: Option<String>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_auth_spec_serialization_roundtrip() {
		// Arrange
		let spec = AuthSpec {
			jwt: true,
			oauth: Some(OAuthSpec {
				provider: "github".to_string(),
				credentials_secret: Some("oauth-secret".to_string()),
			}),
		};
		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let deserialized: AuthSpec = serde_json::from_str(&json).unwrap();
		// Assert
		assert!(deserialized.jwt);
		let oauth = deserialized.oauth.unwrap();
		assert_eq!(oauth.provider, "github");
		assert_eq!(oauth.credentials_secret, Some("oauth-secret".to_string()));
	}

	#[rstest]
	fn test_auth_spec_jwt_defaults_to_false() {
		// Arrange
		let json = r#"{"oauth": null}"#;
		// Act
		let spec: AuthSpec = serde_json::from_str(json).unwrap();
		// Assert
		assert!(!spec.jwt);
	}
}
