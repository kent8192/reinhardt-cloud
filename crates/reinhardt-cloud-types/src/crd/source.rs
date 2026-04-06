//! Git source and CI/CD pipeline types for `ReinhardtApp`.
//!
//! Defines the source specification for linking a ReinhardtApp to a Git
//! repository, including build, webhook, and preview environment configuration.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

/// Git provider for the source repository.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GitProvider {
    GitHub,
    GitLab,
}

/// Webhook event types that trigger pipeline actions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WebhookEvent {
    Push,
    PullRequest,
    Tag,
}

/// Container build configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BuildSpec {
    /// Dockerfile path relative to context.
    pub dockerfile: Option<String>,
    /// Build context directory.
    pub context: Option<String>,
    /// Container registry URL.
    pub registry: Option<String>,
    /// Additional build arguments.
    #[serde(default)]
    pub build_args: BTreeMap<String, String>,
}

impl BuildSpec {
    /// Validates the build specification.
    ///
    /// Checks that `dockerfile` and `context` are non-empty when specified,
    /// and that `registry` is non-empty when specified.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if let Some(ref dockerfile) = self.dockerfile
            && dockerfile.is_empty()
        {
            errors.push(ValidationError::new(
                "source.build.dockerfile must be non-empty when specified",
            ));
        }

        if let Some(ref context) = self.context
            && context.is_empty()
        {
            errors.push(ValidationError::new(
                "source.build.context must be non-empty when specified",
            ));
        }

        if let Some(ref registry) = self.registry
            && registry.is_empty()
        {
            errors.push(ValidationError::new(
                "source.build.registry must be non-empty when specified",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Webhook receiver configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct WebhookSpec {
    /// Whether webhook receiving is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Events to listen for.
    #[serde(default)]
    pub events: Vec<WebhookEvent>,
    /// Secret name containing the webhook signing secret.
    pub secret_ref: Option<String>,
}

impl WebhookSpec {
    /// Validates the webhook specification.
    ///
    /// Checks that `secret_ref` is non-empty when specified, and that
    /// at least one event is configured when enabled.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if let Some(ref secret_ref) = self.secret_ref
            && secret_ref.is_empty()
        {
            errors.push(ValidationError::new(
                "source.webhook.secret_ref must be non-empty when specified",
            ));
        }

        if self.enabled && self.events.is_empty() {
            errors.push(ValidationError::new(
                "source.webhook.events must not be empty when webhook is enabled",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Resource overrides for preview environments.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PreviewOverrides {
    /// Override replica count for preview.
    pub replicas: Option<i32>,
    /// Whether to provision a database for preview.
    pub database: Option<bool>,
    /// Whether to provision a cache for preview.
    pub cache: Option<bool>,
}

/// Preview environment configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PreviewSpec {
    /// Whether preview environments are enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Time-to-live for preview environments (e.g., "72h", "7d").
    pub ttl: Option<String>,
    /// URL template for preview environments.
    /// Supports `{{branch}}` and `{{pr}}` placeholders.
    pub url_template: Option<String>,
    /// Resource overrides for preview deployments.
    pub overrides: Option<PreviewOverrides>,
}

impl PreviewSpec {
    /// Validates the preview specification.
    ///
    /// Checks that `ttl` and `url_template` are non-empty when specified,
    /// and that `overrides.replicas` is positive when set.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if let Some(ref ttl) = self.ttl
            && ttl.is_empty()
        {
            errors.push(ValidationError::new(
                "source.preview.ttl must be non-empty when specified",
            ));
        }

        if let Some(ref url_template) = self.url_template
            && url_template.is_empty()
        {
            errors.push(ValidationError::new(
                "source.preview.url_template must be non-empty when specified",
            ));
        }

        if let Some(ref overrides) = self.overrides
            && let Some(replicas) = overrides.replicas
            && replicas <= 0
        {
            errors.push(ValidationError::new(
                "source.preview.overrides.replicas must be > 0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Git source configuration for a `ReinhardtApp`.
///
/// Links the application to a Git repository and configures CI/CD
/// pipeline behavior including builds, webhooks, and preview environments.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SourceSpec {
    /// Git repository URL.
    pub repository: String,
    /// Branch to track (defaults to "main" at runtime).
    pub branch: Option<String>,
    /// Git hosting provider.
    pub provider: Option<GitProvider>,
    /// Secret name containing Git credentials.
    pub credentials_secret: Option<String>,
    /// Container build configuration.
    pub build: Option<BuildSpec>,
    /// Webhook receiver configuration.
    pub webhook: Option<WebhookSpec>,
    /// Preview environment configuration.
    pub preview: Option<PreviewSpec>,
}

impl SourceSpec {
    /// Validates the source specification.
    ///
    /// Checks that `repository` is non-empty, `branch` and
    /// `credentials_secret` are non-empty when specified, and
    /// delegates to nested spec validations.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if self.repository.is_empty() {
            errors.push(ValidationError::new(
                "source.repository must be non-empty",
            ));
        }

        if let Some(ref branch) = self.branch
            && branch.is_empty()
        {
            errors.push(ValidationError::new(
                "source.branch must be non-empty when specified",
            ));
        }

        if let Some(ref credentials_secret) = self.credentials_secret
            && credentials_secret.is_empty()
        {
            errors.push(ValidationError::new(
                "source.credentials_secret must be non-empty when specified",
            ));
        }

        if let Some(ref build) = self.build
            && let Err(errs) = build.validate()
        {
            errors.extend(errs);
        }

        if let Some(ref webhook) = self.webhook
            && let Err(errs) = webhook.validate()
        {
            errors.extend(errs);
        }

        if let Some(ref preview) = self.preview
            && let Err(errs) = preview.validate()
        {
            errors.extend(errs);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    // --- GitProvider ---

    #[rstest]
    fn git_provider_serialization_roundtrip() {
        // Arrange
        let providers = vec![GitProvider::GitHub, GitProvider::GitLab];

        for provider in providers {
            // Act
            let json = serde_json::to_string(&provider).unwrap();
            let deserialized: GitProvider = serde_json::from_str(&json).unwrap();

            // Assert
            assert_eq!(deserialized, provider);
        }
    }

    #[rstest]
    fn git_provider_serializes_lowercase() {
        // Arrange & Act & Assert
        assert_eq!(serde_json::to_string(&GitProvider::GitHub).unwrap(), r#""github""#);
        assert_eq!(serde_json::to_string(&GitProvider::GitLab).unwrap(), r#""gitlab""#);
    }

    // --- WebhookEvent ---

    #[rstest]
    fn webhook_event_serialization_roundtrip() {
        // Arrange
        let events = vec![WebhookEvent::Push, WebhookEvent::PullRequest, WebhookEvent::Tag];

        for event in events {
            // Act
            let json = serde_json::to_string(&event).unwrap();
            let deserialized: WebhookEvent = serde_json::from_str(&json).unwrap();

            // Assert
            assert_eq!(deserialized, event);
        }
    }

    #[rstest]
    fn webhook_event_serializes_lowercase() {
        // Arrange & Act & Assert
        assert_eq!(serde_json::to_string(&WebhookEvent::Push).unwrap(), r#""push""#);
        assert_eq!(serde_json::to_string(&WebhookEvent::PullRequest).unwrap(), r#""pullrequest""#);
        assert_eq!(serde_json::to_string(&WebhookEvent::Tag).unwrap(), r#""tag""#);
    }

    // --- BuildSpec ---

    #[rstest]
    fn build_spec_valid() {
        // Arrange
        let spec = BuildSpec {
            dockerfile: Some("Dockerfile".to_string()),
            context: Some(".".to_string()),
            registry: Some("ghcr.io/myorg".to_string()),
            build_args: BTreeMap::from([("RUST_VERSION".to_string(), "1.80".to_string())]),
        };

        // Act
        let result = spec.validate();

        // Assert
        assert!(result.is_ok());
    }

    #[rstest]
    fn build_spec_allows_all_none() {
        // Arrange
        let spec = BuildSpec {
            dockerfile: None,
            context: None,
            registry: None,
            build_args: BTreeMap::new(),
        };

        // Act
        let result = spec.validate();

        // Assert
        assert!(result.is_ok());
    }

    #[rstest]
    fn build_spec_rejects_empty_strings() {
        // Arrange
        let spec = BuildSpec {
            dockerfile: Some(String::new()),
            context: Some(String::new()),
            registry: Some(String::new()),
            build_args: BTreeMap::new(),
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 3);
        assert!(errors[0].message.contains("dockerfile"));
        assert!(errors[1].message.contains("context"));
        assert!(errors[2].message.contains("registry"));
    }

    #[rstest]
    fn build_spec_serialization_roundtrip() {
        // Arrange
        let spec = BuildSpec {
            dockerfile: Some("Dockerfile.prod".to_string()),
            context: Some("./app".to_string()),
            registry: Some("ghcr.io/myorg".to_string()),
            build_args: BTreeMap::from([("MODE".to_string(), "release".to_string())]),
        };

        // Act
        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: BuildSpec = serde_json::from_str(&json).unwrap();

        // Assert
        assert_eq!(deserialized, spec);
    }

    // --- WebhookSpec ---

    #[rstest]
    fn webhook_spec_valid() {
        // Arrange
        let spec = WebhookSpec {
            enabled: true,
            events: vec![WebhookEvent::Push, WebhookEvent::PullRequest],
            secret_ref: Some("webhook-secret".to_string()),
        };

        // Act
        let result = spec.validate();

        // Assert
        assert!(result.is_ok());
    }

    #[rstest]
    fn webhook_spec_disabled_no_events_ok() {
        // Arrange
        let spec = WebhookSpec {
            enabled: false,
            events: vec![],
            secret_ref: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        assert!(result.is_ok());
    }

    #[rstest]
    fn webhook_spec_enabled_no_events_rejected() {
        // Arrange
        let spec = WebhookSpec {
            enabled: true,
            events: vec![],
            secret_ref: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("events must not be empty"));
    }

    #[rstest]
    fn webhook_spec_rejects_empty_secret_ref() {
        // Arrange
        let spec = WebhookSpec {
            enabled: false,
            events: vec![],
            secret_ref: Some(String::new()),
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("secret_ref"));
    }

    #[rstest]
    fn webhook_spec_serialization_roundtrip() {
        // Arrange
        let spec = WebhookSpec {
            enabled: true,
            events: vec![WebhookEvent::Push, WebhookEvent::Tag],
            secret_ref: Some("my-secret".to_string()),
        };

        // Act
        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: WebhookSpec = serde_json::from_str(&json).unwrap();

        // Assert
        assert_eq!(deserialized, spec);
    }

    // --- PreviewOverrides ---

    #[rstest]
    fn preview_overrides_serialization_roundtrip() {
        // Arrange
        let overrides = PreviewOverrides {
            replicas: Some(1),
            database: Some(false),
            cache: Some(true),
        };

        // Act
        let json = serde_json::to_string(&overrides).unwrap();
        let deserialized: PreviewOverrides = serde_json::from_str(&json).unwrap();

        // Assert
        assert_eq!(deserialized, overrides);
    }

    // --- PreviewSpec ---

    #[rstest]
    fn preview_spec_valid() {
        // Arrange
        let spec = PreviewSpec {
            enabled: true,
            ttl: Some("72h".to_string()),
            url_template: Some("{{branch}}.preview.example.com".to_string()),
            overrides: Some(PreviewOverrides {
                replicas: Some(1),
                database: Some(false),
                cache: Some(false),
            }),
        };

        // Act
        let result = spec.validate();

        // Assert
        assert!(result.is_ok());
    }

    #[rstest]
    fn preview_spec_rejects_empty_ttl() {
        // Arrange
        let spec = PreviewSpec {
            enabled: true,
            ttl: Some(String::new()),
            url_template: None,
            overrides: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("ttl"));
    }

    #[rstest]
    fn preview_spec_rejects_empty_url_template() {
        // Arrange
        let spec = PreviewSpec {
            enabled: false,
            ttl: None,
            url_template: Some(String::new()),
            overrides: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("url_template"));
    }

    #[rstest]
    fn preview_spec_rejects_zero_replicas() {
        // Arrange
        let spec = PreviewSpec {
            enabled: false,
            ttl: None,
            url_template: None,
            overrides: Some(PreviewOverrides {
                replicas: Some(0),
                database: None,
                cache: None,
            }),
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("replicas must be > 0"));
    }

    #[rstest]
    fn preview_spec_rejects_negative_replicas() {
        // Arrange
        let spec = PreviewSpec {
            enabled: false,
            ttl: None,
            url_template: None,
            overrides: Some(PreviewOverrides {
                replicas: Some(-1),
                database: None,
                cache: None,
            }),
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("replicas must be > 0"));
    }

    #[rstest]
    fn preview_spec_collects_multiple_errors() {
        // Arrange
        let spec = PreviewSpec {
            enabled: false,
            ttl: Some(String::new()),
            url_template: Some(String::new()),
            overrides: Some(PreviewOverrides {
                replicas: Some(0),
                database: None,
                cache: None,
            }),
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 3);
    }

    #[rstest]
    fn preview_spec_serialization_roundtrip() {
        // Arrange
        let spec = PreviewSpec {
            enabled: true,
            ttl: Some("7d".to_string()),
            url_template: Some("{{pr}}.dev.example.com".to_string()),
            overrides: Some(PreviewOverrides {
                replicas: Some(2),
                database: Some(true),
                cache: Some(false),
            }),
        };

        // Act
        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: PreviewSpec = serde_json::from_str(&json).unwrap();

        // Assert
        assert_eq!(deserialized, spec);
    }

    // --- SourceSpec ---

    #[rstest]
    fn source_spec_minimal_valid() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: None,
            provider: None,
            credentials_secret: None,
            build: None,
            webhook: None,
            preview: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        assert!(result.is_ok());
    }

    #[rstest]
    fn source_spec_full_valid() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: Some("develop".to_string()),
            provider: Some(GitProvider::GitHub),
            credentials_secret: Some("git-creds".to_string()),
            build: Some(BuildSpec {
                dockerfile: Some("Dockerfile".to_string()),
                context: Some(".".to_string()),
                registry: Some("ghcr.io/myorg".to_string()),
                build_args: BTreeMap::from([("MODE".to_string(), "release".to_string())]),
            }),
            webhook: Some(WebhookSpec {
                enabled: true,
                events: vec![WebhookEvent::Push, WebhookEvent::PullRequest],
                secret_ref: Some("webhook-secret".to_string()),
            }),
            preview: Some(PreviewSpec {
                enabled: true,
                ttl: Some("72h".to_string()),
                url_template: Some("{{branch}}.preview.example.com".to_string()),
                overrides: Some(PreviewOverrides {
                    replicas: Some(1),
                    database: Some(false),
                    cache: Some(false),
                }),
            }),
        };

        // Act
        let result = spec.validate();

        // Assert
        assert!(result.is_ok());
    }

    #[rstest]
    fn source_spec_rejects_empty_repository() {
        // Arrange
        let spec = SourceSpec {
            repository: String::new(),
            branch: None,
            provider: None,
            credentials_secret: None,
            build: None,
            webhook: None,
            preview: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("repository"));
    }

    #[rstest]
    fn source_spec_rejects_empty_branch() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: Some(String::new()),
            provider: None,
            credentials_secret: None,
            build: None,
            webhook: None,
            preview: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("branch"));
    }

    #[rstest]
    fn source_spec_rejects_empty_credentials_secret() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: None,
            provider: None,
            credentials_secret: Some(String::new()),
            build: None,
            webhook: None,
            preview: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("credentials_secret"));
    }

    #[rstest]
    fn source_spec_delegates_build_validation() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: None,
            provider: None,
            credentials_secret: None,
            build: Some(BuildSpec {
                dockerfile: Some(String::new()),
                context: None,
                registry: None,
                build_args: BTreeMap::new(),
            }),
            webhook: None,
            preview: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("dockerfile"));
    }

    #[rstest]
    fn source_spec_delegates_webhook_validation() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: None,
            provider: None,
            credentials_secret: None,
            build: None,
            webhook: Some(WebhookSpec {
                enabled: true,
                events: vec![],
                secret_ref: None,
            }),
            preview: None,
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("events"));
    }

    #[rstest]
    fn source_spec_delegates_preview_validation() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: None,
            provider: None,
            credentials_secret: None,
            build: None,
            webhook: None,
            preview: Some(PreviewSpec {
                enabled: false,
                ttl: Some(String::new()),
                url_template: None,
                overrides: None,
            }),
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("ttl"));
    }

    #[rstest]
    fn source_spec_collects_all_errors() {
        // Arrange
        let spec = SourceSpec {
            repository: String::new(),
            branch: Some(String::new()),
            provider: None,
            credentials_secret: Some(String::new()),
            build: Some(BuildSpec {
                dockerfile: Some(String::new()),
                context: None,
                registry: None,
                build_args: BTreeMap::new(),
            }),
            webhook: Some(WebhookSpec {
                enabled: true,
                events: vec![],
                secret_ref: Some(String::new()),
            }),
            preview: Some(PreviewSpec {
                enabled: false,
                ttl: Some(String::new()),
                url_template: Some(String::new()),
                overrides: Some(PreviewOverrides {
                    replicas: Some(0),
                    database: None,
                    cache: None,
                }),
            }),
        };

        // Act
        let result = spec.validate();

        // Assert
        let errors = result.unwrap_err();
        // repository(1) + branch(1) + credentials_secret(1) + dockerfile(1) +
        // secret_ref(1) + events(1) + ttl(1) + url_template(1) + replicas(1) = 9
        assert_eq!(errors.len(), 9);
    }

    #[rstest]
    fn source_spec_serialization_roundtrip() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: Some("main".to_string()),
            provider: Some(GitProvider::GitHub),
            credentials_secret: Some("git-token".to_string()),
            build: Some(BuildSpec {
                dockerfile: Some("Dockerfile".to_string()),
                context: Some(".".to_string()),
                registry: Some("ghcr.io/myorg".to_string()),
                build_args: BTreeMap::from([("PROFILE".to_string(), "release".to_string())]),
            }),
            webhook: Some(WebhookSpec {
                enabled: true,
                events: vec![WebhookEvent::Push, WebhookEvent::Tag],
                secret_ref: Some("wh-secret".to_string()),
            }),
            preview: Some(PreviewSpec {
                enabled: true,
                ttl: Some("48h".to_string()),
                url_template: Some("{{branch}}.dev.example.com".to_string()),
                overrides: Some(PreviewOverrides {
                    replicas: Some(1),
                    database: Some(true),
                    cache: Some(false),
                }),
            }),
        };

        // Act
        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: SourceSpec = serde_json::from_str(&json).unwrap();

        // Assert
        assert_eq!(deserialized, spec);
    }

    #[rstest]
    fn source_spec_yaml_roundtrip() {
        // Arrange
        let spec = SourceSpec {
            repository: "https://github.com/myorg/myapp".to_string(),
            branch: Some("main".to_string()),
            provider: Some(GitProvider::GitLab),
            credentials_secret: None,
            build: None,
            webhook: None,
            preview: None,
        };

        // Act
        let yaml = serde_yaml::to_string(&spec).unwrap();
        let deserialized: SourceSpec = serde_yaml::from_str(&yaml).unwrap();

        // Assert
        assert_eq!(deserialized, spec);
    }

    #[rstest]
    fn source_spec_from_json_minimal() {
        // Arrange
        let json = r#"{"repository": "https://github.com/myorg/myapp"}"#;

        // Act
        let spec: SourceSpec = serde_json::from_str(json).unwrap();

        // Assert
        assert_eq!(spec.repository, "https://github.com/myorg/myapp");
        assert!(spec.branch.is_none());
        assert!(spec.provider.is_none());
        assert!(spec.credentials_secret.is_none());
        assert!(spec.build.is_none());
        assert!(spec.webhook.is_none());
        assert!(spec.preview.is_none());
    }
}
