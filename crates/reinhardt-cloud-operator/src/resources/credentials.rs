//! Git credential Secret validation for source-driven builds.

use reinhardt_cloud_types::crd::ReinhardtApp;

/// Returns the credentials Secret name if one is referenced in source spec.
pub(crate) fn should_warn_missing_credentials(app: &ReinhardtApp) -> Option<String> {
	let source = app.spec.source.as_ref()?;
	let secret_name = source.credentials_secret.as_ref()?;
	Some(secret_name.clone())
}

/// Returns the webhook secret name from the source spec, if configured.
pub(crate) fn webhook_secret_name(app: &ReinhardtApp) -> Option<String> {
	app.spec.source.as_ref()?.webhook.as_ref()?.secret_ref.clone()
}

/// Returns the credentials Secret name for the source spec.
pub(crate) fn credentials_secret_name(app: &ReinhardtApp) -> Option<String> {
	app.spec.source.as_ref()?.credentials_secret.clone()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn test_app(name: &str) -> ReinhardtApp {
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": name, "namespace": "default", "uid": "test-uid" },
			"spec": { "image": "myapp:latest" }
		});
		serde_json::from_value(json).unwrap()
	}

	fn test_app_with_source(name: &str, creds: Option<&str>) -> ReinhardtApp {
		let mut app = test_app(name);
		app.spec.source = Some(reinhardt_cloud_types::crd::source::SourceSpec {
			repository: "https://github.com/myorg/myapp".to_string(),
			branch: None,
			provider: None,
			credentials_secret: creds.map(|s| s.to_string()),
			build: None,
			webhook: None,
			preview: None,
		});
		app
	}

	#[test]
	fn should_warn_with_credentials_returns_some() {
		let app = test_app_with_source("myapp", Some("git-creds"));
		assert_eq!(
			should_warn_missing_credentials(&app),
			Some("git-creds".to_string()),
		);
	}

	#[test]
	fn should_warn_without_credentials_returns_none() {
		let app = test_app_with_source("myapp", None);
		assert_eq!(should_warn_missing_credentials(&app), None);
	}

	#[test]
	fn should_warn_without_source_returns_none() {
		let app = test_app("myapp");
		assert_eq!(should_warn_missing_credentials(&app), None);
	}

	#[test]
	fn webhook_secret_with_webhook_returns_some() {
		let mut app = test_app_with_source("myapp", None);
		app.spec.source.as_mut().unwrap().webhook =
			Some(reinhardt_cloud_types::crd::source::WebhookSpec {
				enabled: true,
				events: vec![],
				secret_ref: Some("webhook-secret".to_string()),
			});
		assert_eq!(
			webhook_secret_name(&app),
			Some("webhook-secret".to_string()),
		);
	}

	#[test]
	fn webhook_secret_without_webhook_returns_none() {
		let app = test_app_with_source("myapp", None);
		assert_eq!(webhook_secret_name(&app), None);
	}
}
