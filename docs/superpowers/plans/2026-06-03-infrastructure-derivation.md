# Infrastructure Derivation Implementation Plan

**Goal:** Derive fail-early `spec.infrastructure` baselines from Reinhardt introspection/settings and use them as the Terraform input contract.

**Architecture:** Put derivation in `reinhardt-cloud-core` as pure functions returning `Result<InfrastructureSpec, DerivationError>`. CLI commands call those functions from `deploy`, `sync`, and the `terraform generate` compatibility fallback. The operator stays unchanged.

**Tech Stack:** Rust 2024, `reinhardt-cloud-types`, `reinhardt-cloud-core`, `reinhardt-cloud-cli`, serde YAML/TOML, rstest.

---

## File Structure

- Create `crates/reinhardt-cloud-core/src/infrastructure_derivation.rs`
  - Owns derivation inputs, fail-early errors, Postgres/bucket derivation, explicit-preserving merge helpers, and unit tests.
- Modify `crates/reinhardt-cloud-core/src/lib.rs`
  - Exposes the new module.
- Modify `crates/reinhardt-cloud-types/src/reinhardt_cloud_toml.rs`
  - Adds `infrastructure: Option<InfrastructureSpec>` to the TOML schema and maps it into `ReinhardtAppSpec`.
- Modify `crates/reinhardt-cloud-cli/src/toml_generator.rs`
  - Generates a baseline infrastructure section during `init`/`sync` config generation and returns derivation errors instead of hiding them.
- Modify `crates/reinhardt-cloud-cli/src/commands/init.rs`
  - Propagates fail-early infrastructure derivation errors from config generation.
- Modify `crates/reinhardt-cloud-cli/src/commands/deploy.rs`
  - Derives `spec.infrastructure` when absent and fails early on unsupported managed inputs.
- Modify `crates/reinhardt-cloud-cli/src/commands/sync.rs`
  - Merges existing TOML infrastructure section-by-section while filling missing baseline sections.
- Modify `crates/reinhardt-cloud-cli/src/commands/terraform.rs`
  - Adds `--app-crd` manifest input, prefers explicit infrastructure, and falls back to introspect-derived infrastructure with warning.
- Modify `docs/tools/cli.md`
  - Documents the new contract, fail-early behavior, and fallback.

---

### Task 1: Add Core Derivation Module

**Files:**
- Create: `crates/reinhardt-cloud-core/src/infrastructure_derivation.rs`
- Modify: `crates/reinhardt-cloud-core/src/lib.rs`

- [ ] **Step 1: Write failing core tests**

Create `crates/reinhardt-cloud-core/src/infrastructure_derivation.rs` with the tests first:

```rust
use reinhardt_cloud_types::crd::infrastructure::{
	BucketSpec, InfrastructureSpec, PostgresSpec, SecretSpec,
};
use reinhardt_cloud_types::introspect::InfraSignals;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct InfrastructureDerivationInput {
	pub app_name: String,
	pub signals: InfraSignals,
	pub explicit: Option<InfrastructureSpec>,
	pub typed_secret_refs: Vec<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DerivationError {
	#[error("unsupported managed database engine `{engine}`; supported values: postgres, postgresql")]
	UnsupportedDatabaseEngine { engine: String },
	#[error("unsupported managed storage backend `{backend}`; supported values: s3, gcs")]
	UnsupportedStorageBackend { backend: String },
	#[error("invalid derived infrastructure: {message}")]
	InvalidInfrastructure { message: String },
}

pub fn derive_infrastructure_spec(
	_input: InfrastructureDerivationInput,
) -> Result<Option<InfrastructureSpec>, DerivationError> {
	Ok(None)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn input(app_name: &str, signals: InfraSignals) -> InfrastructureDerivationInput {
		InfrastructureDerivationInput {
			app_name: app_name.to_string(),
			signals,
			explicit: None,
			typed_secret_refs: Vec::new(),
		}
	}

	#[rstest]
	#[case("postgres")]
	#[case("postgresql")]
	fn derives_postgres_for_supported_engines(#[case] engine: &str) {
		let spec = derive_infrastructure_spec(input(
			"orders",
			InfraSignals {
				database: Some(engine.to_string()),
				..Default::default()
			},
		))
		.expect("supported engine should derive")
		.expect("infrastructure should be present");

		let postgres = spec.postgres.expect("postgres should be derived");
		assert_eq!(postgres.version.as_deref(), Some("16"));
		assert_eq!(postgres.backup_retention_days, Some(7));
		assert_eq!(postgres.tier, None);
	}

	#[rstest]
	#[case("mysql")]
	#[case("sqlite")]
	fn rejects_unsupported_database_engines(#[case] engine: &str) {
		let err = derive_infrastructure_spec(input(
			"orders",
			InfraSignals {
				database: Some(engine.to_string()),
				..Default::default()
			},
		))
		.expect_err("unsupported engine should fail early");

		assert_eq!(
			err,
			DerivationError::UnsupportedDatabaseEngine {
				engine: engine.to_string()
			}
		);
	}

	#[rstest]
	#[case("s3")]
	#[case("gcs")]
	fn derives_bucket_for_supported_storage(#[case] backend: &str) {
		let spec = derive_infrastructure_spec(input(
			"orders",
			InfraSignals {
				storage: Some(backend.to_string()),
				..Default::default()
			},
		))
		.expect("supported storage should derive")
		.expect("infrastructure should be present");

		let buckets = spec.buckets.expect("bucket should be derived");
		assert_eq!(buckets.len(), 1);
		assert_eq!(buckets[0].name, "orders-assets");
		assert!(!buckets[0].public);
	}

	#[rstest]
	#[case("local")]
	#[case("pvc")]
	#[case("minio")]
	fn rejects_unsupported_storage_backends(#[case] backend: &str) {
		let err = derive_infrastructure_spec(input(
			"orders",
			InfraSignals {
				storage: Some(backend.to_string()),
				..Default::default()
			},
		))
		.expect_err("unsupported storage should fail early");

		assert_eq!(
			err,
			DerivationError::UnsupportedStorageBackend {
				backend: backend.to_string()
			}
		);
	}

	#[rstest]
	fn returns_none_when_no_managed_signals_exist() {
		let spec = derive_infrastructure_spec(input("orders", InfraSignals::default()))
			.expect("empty signals should not fail");

		assert!(spec.is_none());
	}

	#[rstest]
	fn preserves_explicit_infrastructure() {
		let explicit = InfrastructureSpec {
			postgres: Some(PostgresSpec {
				tier: Some("db-custom-2-4096".to_string()),
				version: Some("15".to_string()),
				backup_retention_days: Some(14),
			}),
			buckets: Some(vec![BucketSpec {
				name: "custom-assets".to_string(),
				public: true,
			}]),
			dns: None,
			secrets: None,
		};
		let spec = derive_infrastructure_spec(InfrastructureDerivationInput {
			app_name: "orders".to_string(),
			signals: InfraSignals {
				database: Some("postgres".to_string()),
				storage: Some("s3".to_string()),
				..Default::default()
			},
			explicit: Some(explicit.clone()),
			typed_secret_refs: Vec::new(),
		})
		.expect("explicit infrastructure should validate")
		.expect("explicit infrastructure should be returned");

		assert_eq!(spec, explicit);
	}

	#[rstest]
	fn derives_typed_secret_refs() {
		let spec = derive_infrastructure_spec(InfrastructureDerivationInput {
			app_name: "orders".to_string(),
			signals: InfraSignals::default(),
			explicit: None,
			typed_secret_refs: vec![
				"git-creds".to_string(),
				"webhook-secret".to_string(),
				"git-creds".to_string(),
			],
		})
		.expect("typed secret refs should derive")
		.expect("infrastructure should be present");

		assert_eq!(
			spec.secrets,
			Some(vec![
				SecretSpec {
					name: "git-creds".to_string(),
					description: Some("Application-managed secret reference".to_string()),
				},
				SecretSpec {
					name: "webhook-secret".to_string(),
					description: Some("Application-managed secret reference".to_string()),
				},
			])
		);
	}
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p reinhardt-cloud-core infrastructure_derivation
```

Expected: FAIL because supported Postgres and storage signals return `None`
instead of a derived infrastructure spec.

- [ ] **Step 3: Implement derivation**

Replace the function body and add helpers in `crates/reinhardt-cloud-core/src/infrastructure_derivation.rs`:

```rust
pub fn derive_infrastructure_spec(
	input: InfrastructureDerivationInput,
) -> Result<Option<InfrastructureSpec>, DerivationError> {
	if let Some(explicit) = input.explicit {
		validate(&explicit)?;
		return Ok(Some(explicit));
	}

	let mut spec = InfrastructureSpec::default();

	if let Some(engine) = input.signals.database.as_deref() {
		match engine {
			"postgres" | "postgresql" => {
				spec.postgres = Some(PostgresSpec {
					tier: None,
					version: Some("16".to_string()),
					backup_retention_days: Some(7),
				});
			}
			unsupported => {
				return Err(DerivationError::UnsupportedDatabaseEngine {
					engine: unsupported.to_string(),
				});
			}
		}
	}

	if let Some(backend) = input.signals.storage.as_deref() {
		match backend {
			"s3" | "gcs" => {
				spec.buckets = Some(vec![BucketSpec {
					name: format!("{}-assets", input.app_name),
					public: false,
				}]);
			}
			unsupported => {
				return Err(DerivationError::UnsupportedStorageBackend {
					backend: unsupported.to_string(),
				});
			}
		}
	}

	let mut secret_refs = input.typed_secret_refs;
	secret_refs.sort();
	secret_refs.dedup();
	let secrets = secret_refs
		.into_iter()
		.filter(|name| !name.trim().is_empty())
		.map(|name| SecretSpec {
			name,
			description: Some("Application-managed secret reference".to_string()),
		})
		.collect::<Vec<_>>();
	if !secrets.is_empty() {
		spec.secrets = Some(secrets);
	}

	let has_resources =
		spec.postgres.is_some() || spec.buckets.is_some() || spec.dns.is_some() || spec.secrets.is_some();
	if !has_resources {
		return Ok(None);
	}

	validate(&spec)?;
	Ok(Some(spec))
}

fn validate(spec: &InfrastructureSpec) -> Result<(), DerivationError> {
	spec.validate().map_err(|errors| DerivationError::InvalidInfrastructure {
		message: errors
			.into_iter()
			.map(|error| error.message)
			.collect::<Vec<_>>()
			.join("; "),
	})
}
```

- [ ] **Step 4: Export module**

Add to `crates/reinhardt-cloud-core/src/lib.rs`:

```rust
pub mod infrastructure_derivation;
```

- [ ] **Step 5: Run tests and verify pass**

Run:

```bash
cargo test -p reinhardt-cloud-core infrastructure_derivation
```

Expected: PASS for all infrastructure derivation tests.

---

### Task 2: Add Infrastructure to `reinhardt-cloud.toml`

**Files:**
- Modify: `crates/reinhardt-cloud-types/src/reinhardt_cloud_toml.rs`

- [ ] **Step 1: Write failing TOML schema tests**

Add imports near the existing `crate::crd` import:

```rust
use crate::crd::InfrastructureSpec;
```

Add tests in the existing `#[cfg(test)] mod tests`:

```rust
#[rstest]
fn test_parse_infrastructure_section() {
	let toml_str = r#"
[app]
name = "infra-app"
image = "infra-app:latest"

[infrastructure.postgres]
version = "16"
backup_retention_days = 7

[[infrastructure.buckets]]
name = "infra-app-assets"
public = false
"#;

	let config: ReinhardtCloudToml = toml::from_str(toml_str).unwrap();

	let infrastructure = config.infrastructure.expect("infrastructure should parse");
	assert!(infrastructure.postgres.is_some());
	assert_eq!(
		infrastructure.buckets.as_ref().unwrap()[0].name,
		"infra-app-assets"
	);
}

#[rstest]
fn test_reinhardt_cloud_toml_to_spec_preserves_infrastructure() {
	let config = ReinhardtCloudToml {
		app: AppSection {
			name: "infra-app".into(),
			image: "infra-app:latest".into(),
		},
		infrastructure: Some(InfrastructureSpec {
			postgres: Some(crate::crd::PostgresSpec {
				tier: None,
				version: Some("16".into()),
				backup_retention_days: Some(7),
			}),
			buckets: None,
			dns: None,
			secrets: None,
		}),
		..Default::default()
	};

	let spec = config.to_reinhardt_app_spec();

	assert_eq!(spec.infrastructure, config.infrastructure);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p reinhardt-cloud-types reinhardt_cloud_toml
```

Expected: FAIL because `ReinhardtCloudToml` has no `infrastructure` field.

- [ ] **Step 3: Add TOML field and spec mapping**

In `ReinhardtCloudToml`, add after `mail`:

```rust
/// Per-application managed cloud infrastructure used by Terraform generation
#[serde(default)]
pub infrastructure: Option<InfrastructureSpec>,
```

In `to_reinhardt_app_spec`, replace:

```rust
infrastructure: None,
```

with:

```rust
infrastructure: self.infrastructure.clone(),
```

- [ ] **Step 4: Run tests and verify pass**

Run:

```bash
cargo test -p reinhardt-cloud-types reinhardt_cloud_toml
```

Expected: PASS.

---

### Task 3: Generate Baseline Infrastructure in TOML Config

**Files:**
- Modify: `crates/reinhardt-cloud-cli/src/toml_generator.rs`
- Modify: `crates/reinhardt-cloud-cli/src/commands/init.rs`
- Modify: `crates/reinhardt-cloud-cli/src/commands/sync.rs`
- Modify: `crates/reinhardt-cloud-cli/Cargo.toml`

- [ ] **Step 1: Write failing generator tests**

Add imports at the top:

```rust
use reinhardt_cloud_core::infrastructure_derivation::{
	derive_infrastructure_spec, InfrastructureDerivationInput,
};
```

Add tests in `toml_generator.rs`:

```rust
#[rstest]
fn test_generate_config_derives_postgres_infrastructure() {
	let metadata = ProjectMetadata {
		name: "orders".into(),
		version: "0.1.0".into(),
		features: vec!["db-postgres".into()],
		signals: InfraSignals {
			database: Some("postgresql".into()),
			..Default::default()
		},
	};

	let config = generate_config(&metadata, None).expect("supported signals should derive");

	let infrastructure = config.infrastructure.expect("infrastructure should be generated");
	assert!(infrastructure.postgres.is_some());
}

#[rstest]
fn test_generate_config_does_not_derive_bucket_from_boolean_storage_signal() {
	let metadata = ProjectMetadata {
		name: "assets".into(),
		version: "0.1.0".into(),
		features: vec!["storage".into()],
		signals: InfraSignals {
			object_storage: true,
			..Default::default()
		},
	};

	let config = generate_config(&metadata, None).expect("supported signals should derive");

	assert!(
		config
			.infrastructure
			.as_ref()
			.and_then(|infra| infra.buckets.as_ref())
			.is_none()
	);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p reinhardt-cloud-cli toml_generator
```

Expected: FAIL because `generate_config` still returns `ReinhardtCloudToml`
directly and does not set Postgres infrastructure.

- [ ] **Step 3: Add core dependency**

Add to `crates/reinhardt-cloud-cli/Cargo.toml`:

```toml
reinhardt-cloud-core = { workspace = true }
```

- [ ] **Step 4: Change `generate_config` to fail early**

Change the signature:

```rust
pub(crate) fn generate_config(
	metadata: &ProjectMetadata,
	db_config: Option<&DatabaseConfig>,
) -> Result<ReinhardtCloudToml, String> {
```

At the end of the function, return:

```rust
Ok(ReinhardtCloudToml {
	app: AppSection {
		name: metadata.name.clone(),
		image: format!("{}:latest", metadata.name),
	},
	database: if metadata.signals.database.is_some() || db_config.is_some() {
		Some(DatabaseSection {
			engine: db_engine,
			..Default::default()
		})
	} else {
		None
	},
	auth: if metadata.signals.jwt {
		Some(AuthSection { jwt: true })
	} else {
		None
	},
	cache: metadata.signals.cache.as_ref().map(|backend| CacheSection {
		backend: backend.clone(),
		..Default::default()
	}),
	worker: if metadata.signals.background_worker {
		Some(WorkerSection::default())
	} else {
		None
	},
	storage: if metadata.signals.object_storage {
		Some(StorageSection::default())
	} else {
		None
	},
	infrastructure,
	..Default::default()
})
```

- [ ] **Step 5: Add signal conversion helper**

In `toml_generator.rs`, add:

```rust
fn to_introspect_signals(signals: &InfraSignals) -> reinhardt_cloud_types::introspect::InfraSignals {
	reinhardt_cloud_types::introspect::InfraSignals {
		database: signals.database.clone(),
		cache: signals.cache.clone(),
		websocket: signals.websocket,
		background_worker: signals.background_worker,
		grpc: signals.grpc,
		// `feature_detector::InfraSignals::object_storage` is only a boolean.
		// It does not say whether the desired managed backend is S3 or GCS, so
		// do not synthesize a Terraform bucket from it.
		storage: None,
		mail: None,
		session_backend: signals.sessions.then(|| "db".to_string()),
		graphql: signals.graphql,
		admin_panel: false,
		i18n: false,
		pages: signals.pages,
	}
}
```

- [ ] **Step 6: Derive infrastructure in `generate_config`**

Before returning `ReinhardtCloudToml`, compute:

```rust
let infrastructure = derive_infrastructure_spec(InfrastructureDerivationInput {
	app_name: metadata.name.clone(),
	signals: to_introspect_signals(&metadata.signals),
	explicit: None,
	typed_secret_refs: Vec::new(),
})
.map_err(|e| e.to_string())?;
```

Set the returned config field:

```rust
infrastructure,
```

- [ ] **Step 7: Update existing generator tests**

Every existing `toml_generator.rs` test that calls `generate_config` must unwrap the `Result`:

```rust
let config = generate_config(&metadata, None).expect("config generation should succeed");
```

For the database-config override test:

```rust
let config = generate_config(&metadata, Some(&db_config))
	.expect("config generation should succeed");
```

- [ ] **Step 8: Update init and sync call sites**

In `crates/reinhardt-cloud-cli/src/commands/init.rs`, replace:

```rust
let config = generate_config(&metadata, db_config.as_ref());
```

with:

```rust
let config = generate_config(&metadata, db_config.as_ref())?;
```

In `crates/reinhardt-cloud-cli/src/commands/sync.rs`, replace:

```rust
let config = generate_config(&metadata, db_config.as_ref());
```

with:

```rust
let config = generate_config(&metadata, db_config.as_ref())?;
```

- [ ] **Step 9: Run tests and verify pass**

Run:

```bash
cargo test -p reinhardt-cloud-cli toml_generator
```

Expected: PASS.

---

### Task 4: Wire Fail-Early Derivation into Deploy

**Files:**
- Modify: `crates/reinhardt-cloud-cli/src/commands/deploy.rs`

- [ ] **Step 1: Write failing deploy spec tests**

Add tests near existing `test_build_reinhardt_app_crd_with_introspect`:

```rust
#[rstest]
fn test_build_reinhardt_app_spec_derives_infrastructure_from_introspect() {
	let introspect = IntrospectOutput {
		app: AppMetadata {
			name: "orders".to_string(),
			version: "1.0.0".to_string(),
		},
		features: FeaturesMetadata {
			infrastructure_signals: InfraSignals {
				database: Some("postgres".to_string()),
				storage: Some("s3".to_string()),
				..Default::default()
			},
			..Default::default()
		},
		..Default::default()
	};

	let spec = build_reinhardt_app_spec(None, "orders:v1".to_string(), 2, Some(introspect))
		.expect("supported signals should derive");

	let infrastructure = spec.infrastructure.expect("infrastructure should be derived");
	assert!(infrastructure.postgres.is_some());
	assert_eq!(
		infrastructure.buckets.as_ref().unwrap()[0].name,
		"orders-assets"
	);
}

#[rstest]
fn test_build_reinhardt_app_spec_fails_on_unsupported_storage() {
	let introspect = IntrospectOutput {
		app: AppMetadata {
			name: "orders".to_string(),
			version: "1.0.0".to_string(),
		},
		features: FeaturesMetadata {
			infrastructure_signals: InfraSignals {
				storage: Some("local".to_string()),
				..Default::default()
			},
			..Default::default()
		},
		..Default::default()
	};

	let err = build_reinhardt_app_spec(None, "orders:v1".to_string(), 2, Some(introspect))
		.expect_err("unsupported storage should fail early");

	assert!(err.contains("unsupported managed storage backend `local`"));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p reinhardt-cloud-cli commands::deploy
```

Expected: FAIL because `build_reinhardt_app_spec` returns `ReinhardtAppSpec`, not `Result`.

- [ ] **Step 3: Change function signature**

Change:

```rust
fn build_reinhardt_app_spec(
	toml_config: Option<&ReinhardtCloudToml>,
	image: String,
	replicas: i32,
	introspect: Option<IntrospectOutput>,
) -> ReinhardtAppSpec {
```

to:

```rust
fn build_reinhardt_app_spec(
	toml_config: Option<&ReinhardtCloudToml>,
	image: String,
	replicas: i32,
	introspect: Option<IntrospectOutput>,
) -> Result<ReinhardtAppSpec, String> {
```

- [ ] **Step 4: Add derivation call**

Inside `build_reinhardt_app_spec`, after `spec.introspect = introspect;`, add:

```rust
if spec.infrastructure.is_none()
	&& let Some(ref introspect) = spec.introspect
{
	spec.infrastructure = reinhardt_cloud_core::infrastructure_derivation::derive_infrastructure_spec(
		reinhardt_cloud_core::infrastructure_derivation::InfrastructureDerivationInput {
			app_name: introspect.app.name.clone(),
			signals: introspect.features.infrastructure_signals.clone(),
			explicit: None,
			typed_secret_refs: typed_secret_refs(&spec),
		},
	)
	.map_err(|e| e.to_string())?;
}

Ok(spec)
```

Add helper in the same file:

```rust
fn typed_secret_refs(spec: &ReinhardtAppSpec) -> Vec<String> {
	let mut refs = Vec::new();
	if let Some(source) = &spec.source {
		if let Some(secret) = &source.credentials_secret {
			refs.push(secret.clone());
		}
		if let Some(webhook) = &source.webhook
			&& let Some(secret) = &webhook.secret_ref
		{
			refs.push(secret.clone());
		}
	}
	if let Some(mail) = &spec.mail
		&& let Some(secret) = &mail.credentials_secret
	{
		refs.push(secret.clone());
	}
	refs
}
```

- [ ] **Step 5: Update call sites**

In `execute_inner`, replace:

```rust
let spec = build_reinhardt_app_spec(
	toml_config.as_ref(),
	image.clone(),
	replicas_i32,
	introspect,
);
```

with:

```rust
let spec = build_reinhardt_app_spec(
	toml_config.as_ref(),
	image.clone(),
	replicas_i32,
	introspect,
)?;
```

In tests, add `.expect("spec should build")` to existing successful call sites.

- [ ] **Step 6: Run deploy tests**

Run:

```bash
cargo test -p reinhardt-cloud-cli commands::deploy
```

Expected: PASS.

---

### Task 5: Preserve Existing Infrastructure During Sync

**Files:**
- Modify: `crates/reinhardt-cloud-cli/src/commands/sync.rs`

- [ ] **Step 1: Write failing sync helper tests**

Add a helper and tests under `#[cfg(test)] mod tests` in `sync.rs`:

```rust
#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::crd::infrastructure::{InfrastructureSpec, PostgresSpec};
	use reinhardt_cloud_types::reinhardt_cloud_toml::{AppSection, ReinhardtCloudToml};
	use rstest::rstest;

	#[rstest]
	fn merge_preserves_existing_infrastructure_sections() {
		let existing = ReinhardtCloudToml {
			app: AppSection {
				name: "orders".to_string(),
				image: "orders:latest".to_string(),
			},
			infrastructure: Some(InfrastructureSpec {
				postgres: Some(PostgresSpec {
					tier: Some("db-custom-2-4096".to_string()),
					version: Some("15".to_string()),
					backup_retention_days: Some(14),
				}),
				buckets: None,
				dns: None,
				secrets: None,
			}),
			..Default::default()
		};
		let mut generated = ReinhardtCloudToml {
			app: existing.app.clone(),
			infrastructure: Some(InfrastructureSpec {
				postgres: Some(PostgresSpec {
					tier: None,
					version: Some("16".to_string()),
					backup_retention_days: Some(7),
				}),
				buckets: Some(vec![reinhardt_cloud_types::crd::BucketSpec {
					name: "orders-assets".to_string(),
					public: false,
				}]),
				dns: None,
				secrets: None,
			}),
			..Default::default()
		};

		merge_existing_infrastructure(&existing, &mut generated);

		let merged = generated.infrastructure.expect("infrastructure should exist");
		assert_eq!(
			merged.postgres,
			existing.infrastructure.unwrap().postgres
		);
		assert_eq!(merged.buckets.unwrap()[0].name, "orders-assets");
	}
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test -p reinhardt-cloud-cli commands::sync::tests::merge_preserves_existing_infrastructure_sections
```

Expected: FAIL because `merge_existing_infrastructure` does not exist.

- [ ] **Step 3: Implement preservation helper**

Add to `sync.rs`:

```rust
fn merge_existing_infrastructure(
	existing: &reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml,
	generated: &mut reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml,
) {
	let Some(existing_infra) = &existing.infrastructure else {
		return;
	};

	let Some(generated_infra) = &mut generated.infrastructure else {
		generated.infrastructure = Some(existing_infra.clone());
		return;
	};

	if existing_infra.postgres.is_some() {
		generated_infra.postgres = existing_infra.postgres.clone();
	}
	if existing_infra.buckets.is_some() {
		generated_infra.buckets = existing_infra.buckets.clone();
	}
	if existing_infra.dns.is_some() {
		generated_infra.dns = existing_infra.dns.clone();
	}
	if existing_infra.secrets.is_some() {
		generated_infra.secrets = existing_infra.secrets.clone();
	}
}
```

- [ ] **Step 4: Use helper in `execute`**

After `let mut config = generate_config(...)?`, read existing TOML and preserve infrastructure:

```rust
let existing_content = std::fs::read_to_string(&reinhardt_cloud_toml_path)?;
let existing_config: reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml =
	toml::from_str(&existing_content)?;
let mut config = generate_config(&metadata, db_config.as_ref())?;
merge_existing_infrastructure(&existing_config, &mut config);
```

Remove the previous immutable `let config = ...` binding.

- [ ] **Step 5: Run sync tests**

Run:

```bash
cargo test -p reinhardt-cloud-cli commands::sync
```

Expected: PASS.

---

### Task 6: Add Terraform Manifest Input and Introspection Fallback

**Files:**
- Modify: `crates/reinhardt-cloud-cli/src/commands/terraform.rs`

- [ ] **Step 1: Write failing extraction tests**

Add tests in `terraform.rs`:

```rust
#[rstest]
fn extracts_explicit_infrastructure_from_crd_yaml() {
	let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: ReinhardtApp
metadata:
  name: orders
spec:
  image: orders:v1
  infrastructure:
    postgres:
      version: "16"
      backup_retention_days: 7
"#;

	let infra = infrastructure_from_crd_yaml("orders", yaml)
		.expect("YAML should parse")
		.expect("infrastructure should exist");

	assert!(infra.postgres.is_some());
}

#[rstest]
fn falls_back_to_introspect_when_infrastructure_missing() {
	let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: ReinhardtApp
metadata:
  name: orders
spec:
  image: orders:v1
  introspect:
    app:
      name: orders
    features:
      infrastructure_signals:
        database: postgres
"#;

	let infra = infrastructure_from_crd_yaml("orders", yaml)
		.expect("fallback should derive")
		.expect("fallback infrastructure should exist");

	assert!(infra.postgres.is_some());
}

#[rstest]
fn fallback_fails_on_unsupported_storage() {
	let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: ReinhardtApp
metadata:
  name: orders
spec:
  image: orders:v1
  introspect:
    app:
      name: orders
    features:
      infrastructure_signals:
        storage: local
"#;

	let err = infrastructure_from_crd_yaml("orders", yaml)
		.expect_err("unsupported fallback should fail early");

	assert!(err.contains("unsupported managed storage backend `local`"));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p reinhardt-cloud-cli commands::terraform
```

Expected: FAIL because `infrastructure_from_crd_yaml` does not exist.

- [ ] **Step 3: Add CLI argument**

In `GenerateArgs`, add:

```rust
/// ReinhardtApp YAML manifest. Preferred source for spec.infrastructure.
#[arg(long)]
pub(crate) app_crd: Option<PathBuf>,
```

- [ ] **Step 4: Add extraction helper**

Add to `terraform.rs`:

```rust
fn infrastructure_from_crd_yaml(
	app: &str,
	yaml: &str,
) -> Result<Option<InfrastructureSpec>, String> {
	let value: serde_yaml::Value =
		serde_yaml::from_str(yaml).map_err(|e| format!("failed to parse ReinhardtApp YAML: {e}"))?;
	let spec_key = serde_yaml::Value::String("spec".to_string());
	let spec_value = value
		.as_mapping()
		.and_then(|mapping| mapping.get(&spec_key))
		.ok_or("ReinhardtApp YAML is missing spec")?
		.clone();
	let spec: reinhardt_cloud_types::crd::ReinhardtAppSpec =
		serde_yaml::from_value(spec_value)
			.map_err(|e| format!("failed to parse ReinhardtApp spec: {e}"))?;

	if spec.infrastructure.is_some() {
		return Ok(spec.infrastructure);
	}

	if let Some(introspect) = spec.introspect {
		eprintln!(
			"warning: spec.infrastructure is absent; deriving from spec.introspect for compatibility. Persist the generated infrastructure block for repeatable Terraform."
		);
		return reinhardt_cloud_core::infrastructure_derivation::derive_infrastructure_spec(
			reinhardt_cloud_core::infrastructure_derivation::InfrastructureDerivationInput {
				app_name: if introspect.app.name.is_empty() {
					app.to_string()
				} else {
					introspect.app.name.clone()
				},
				signals: introspect.features.infrastructure_signals,
				explicit: None,
				typed_secret_refs: Vec::new(),
			},
		)
		.map_err(|e| e.to_string());
	}

	Ok(None)
}
```

- [ ] **Step 5: Use manifest input in `generate`**

Replace the existing infra loading block with:

```rust
let infra: InfrastructureSpec = if let Some(json) = &args.infra_json {
	serde_json::from_str(json)?
} else if let Some(app_crd) = &args.app_crd {
	let yaml = std::fs::read_to_string(app_crd)?;
	infrastructure_from_crd_yaml(&args.app, &yaml)?.unwrap_or_default()
} else {
	InfrastructureSpec::default()
};
```

- [ ] **Step 6: Run terraform tests**

Run:

```bash
cargo test -p reinhardt-cloud-cli commands::terraform
```

Expected: PASS.

---

### Task 7: Update CLI Documentation

**Files:**
- Modify: `docs/tools/cli.md`

- [ ] **Step 1: Inspect Terraform section**

Run:

```bash
rg -n "terraform|infrastructure|deploy|sync" docs/tools/cli.md
```

Expected: Find the current CLI command documentation locations.

- [ ] **Step 2: Add documentation**

Add a section describing this behavior:

```markdown
### Infrastructure Derivation

`spec.infrastructure` is the Terraform input contract for per-application
managed cloud resources. `reinhardt-cloud deploy` and `reinhardt-cloud sync`
derive a baseline infrastructure block from supported introspection/settings
signals when the block is absent.

Supported derived resources:

- PostgreSQL for `postgres` or `postgresql` database signals
- one app-scoped bucket for `s3` or `gcs` storage signals
- typed secret references already declared in Reinhardt Cloud configuration

Unsupported managed signals fail early. For example, `mysql`, `sqlite`,
`local`, `pvc`, and unknown storage backends do not produce guessed Terraform.
Use an explicit `infrastructure` block when the application needs resources that
cannot be safely derived.

`reinhardt-cloud terraform generate --app-crd app.yaml` prefers
`spec.infrastructure`. If the block is absent but `spec.introspect` is present,
the command derives a compatibility baseline and prints a warning. Persist the
generated infrastructure block for repeatable Terraform output.
```

- [ ] **Step 3: Run docs grep**

Run:

```bash
rg -n "Infrastructure Derivation|--app-crd|fail early|spec.infrastructure" docs/tools/cli.md
```

Expected: All new terms are present.

---

### Task 8: Final Verification

**Files:**
- No new files. Verifies the accumulated branch.

- [ ] **Step 1: Format**

Run:

```bash
cargo make fmt-check
```

Expected: PASS.

- [ ] **Step 2: Focused tests**

Run:

```bash
cargo test -p reinhardt-cloud-core infrastructure_derivation
cargo test -p reinhardt-cloud-types reinhardt_cloud_toml
cargo test -p reinhardt-cloud-cli commands::deploy
cargo test -p reinhardt-cloud-cli commands::sync
cargo test -p reinhardt-cloud-cli commands::terraform
cargo test -p reinhardt-cloud-cli toml_generator
```

Expected: PASS.

- [ ] **Step 3: Clippy**

Run:

```bash
cargo make clippy-check
```

Expected: PASS.
