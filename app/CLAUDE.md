# app/CLAUDE.md

## Purpose

This file defines development conventions specific to the `app/` crate, which contains the reinhardt-web application layer (auth, clusters, deployments). These rules supplement the root `CLAUDE.md` and `instructions/` standards.

---

## Migration Management

### MG-1 (MUST): Generate Migrations via Command

- Migrations MUST be generated using `cargo make makemigrations` from the `app/` directory
- Files under `migrations/` MUST NOT be manually edited
- Apply migrations with `cargo make migrate`

---

## Module File Conventions

### MF-1 (MUST): Aggregator-Only Module Files

The following module files MUST contain ONLY `pub mod` declarations and `pub use` re-exports:

- `views.rs`
- `tests.rs`
- `models.rs`
- `serializers.rs`
- `admin.rs`

**NEVER** place helper functions, implementation logic, or any other code in these files.

Each function or type MUST be placed in a separate file within the corresponding directory.

**DO:**

```rust
//! View functions for auth endpoints.

pub mod login;
pub mod register;

pub use login::login;
pub use register::register;
```

**DON'T:**

```rust
//! View functions for auth endpoints.

pub mod login;
pub mod register;

pub use login::login;
pub use register::register;

// Helper function placed directly in the aggregator file
pub(crate) fn jwt_secret() -> String {
	std::env::var("REINHARDT_CLOUD_JWT_SECRET").unwrap_or_else(|_| "default".to_string())
}
```

Helper functions MUST be placed in a dedicated file (e.g., `views/utils.rs`).

---

## Application Creation

### AC-1 (MUST): Use reinhardt-admin

- New applications MUST be created using `reinhardt-admin startapp <app_name>`
- The `--restful` template SHOULD be used for REST API applications
- After creation, register the app in `config/apps.rs`

---

## Test Directory Structure

### TD-1 (MUST): Per-Application Tests

Each application's tests MUST follow this structure:

```
app/src/apps/<app_name>/tests/
├── tests.rs               (module declarations only)
├── e2e/
│   └── test_<specific_feature>.rs
├── integration/
│   └── test_<specific_feature>.rs
└── unit/
    └── test_<specific_feature>.rs
```

- Organize by test type: `unit/`, `e2e/`, `integration/`
- Each test file MUST describe the specific feature being tested

### TD-2 (MUST): Cross-Application Tests

Tests spanning multiple applications belong in `app/tests/`:

```
app/tests/
├── tests.rs                        (module declarations only)
├── smoke_test.rs                   (standalone smoke tests)
├── e2e/
│   ├── <app1>_<app2>/
│   │   └── test_<specific_feature>.rs
│   └── <app1>_<app2>_<app3>/
│       └── test_<specific_feature>.rs
└── integration/
    └── <app1>_<app2>/
        └── test_<specific_feature>.rs
```

- Subdirectory names join related application names with `_`
- Test files MUST be named `test_<descriptive_name>.rs`

### TD-3 (NEVER): Prohibited Test File Names

- **NEVER** use generic names like `e2e_test.rs`, `unit_test.rs`, `integration_test.rs`
- **NEVER** use `e2e_tests.rs`, `unit_tests.rs`, `serializer_tests.rs`
- File names MUST reflect the specific feature under test

**DON'T:**

```
tests/e2e/e2e_tests.rs          # What is being tested?
tests/unit/unit_test.rs          # Meaningless name
tests/serializer_tests.rs        # Too broad
```

**DO:**

```
tests/e2e/test_register_login.rs       # Auth registration and login flow
tests/unit/test_jwt.rs                 # JWT token generation and verification
tests/unit/test_serializer.rs          # Serializer conversion (acceptable when app has one model)
tests/e2e/test_cluster_crud.rs         # Cluster CRUD operations
```

---

## Database Operations

### DB-1 (MUST): ORM-Only Access

- ALL database operations MUST use the reinhardt ORM
- **NEVER** write raw SQL queries
- Use `Model::objects()` with `QuerySet` API for all queries
- Use `FilterOperator` and `FilterValue` for query conditions

---

## Serializer Patterns

### SP-1 (SHOULD): Separate Request and Response Serializers

- Define request serializers (e.g., `CreateClusterRequest`) and response serializers (e.g., `ClusterResponse`) in separate files
- Place them in the `serializers/` directory:

```
serializers/
├── serializers.rs     (pub mod + pub use only)
├── request.rs         (request DTOs)
└── response.rs        (response DTOs)
```

### SP-2 (SHOULD): Use `From<Model>` for Response Conversion

Implement `From<Model>` trait to convert ORM models to response serializers:

```rust
impl From<Cluster> for ClusterResponse {
	fn from(cluster: Cluster) -> Self {
		Self {
			id: cluster.id(),
			name: cluster.name().to_string(),
			api_url: cluster.api_url().to_string(),
			is_active: cluster.is_active(),
		}
	}
}
```

---

## View Patterns

### VP-1 (MUST): One Endpoint Per File

- Each API endpoint MUST be in its own file within the `views/` directory
- File names SHOULD match the endpoint action (e.g., `create_cluster.rs`, `list_clusters.rs`)

### VP-2 (SHOULD): Use `pre_validate = true`

- Use `pre_validate = true` in endpoint macros to enable automatic request validation:

```rust
#[post("/auth/login/", name = "auth_login", pre_validate = true)]
pub async fn login(body: Json<LoginRequest>) -> ViewResult<Response> { ... }
```

### VP-3 (SHOULD): Use Dependency Injection

- Use `#[inject]` or `use_inject = true` for accessing shared resources
- Prefer injection over global state access

---

## Configuration Management

### CM-1 (MUST): Environment-Based Settings

- Use TOML configuration files: `base.toml` for shared settings, environment-specific files for overrides
- Settings files location: `settings/` directory
- **NEVER** hardcode secrets in configuration files or source code

### CM-2 (MUST): Secrets via Environment Variables

- ALL secrets (JWT keys, database credentials, API keys) MUST be loaded from environment variables
- Development fallback values are acceptable ONLY for local development
- Production deployments MUST set all required environment variables

---

## URL Routing

### UR-1 (MUST): Per-App URL Patterns

- Each application MUST define its own `url_patterns()` function in `urls.rs`
- Routes MUST be aggregated in `config/urls.rs` using `.mount()`:

```rust
pub fn routes() -> ServerRouter {
	let mut router = ServerRouter::new();
	router.mount("/api", auth::urls::url_patterns());
	router.mount("/api", clusters::urls::url_patterns());
	router
}
```

---

## Security

### SEC-1 (MUST): JWT Secret Management

- JWT secrets MUST be loaded from `REINHARDT_CLOUD_JWT_SECRET` environment variable
- Production secrets MUST be at least 32 bytes of cryptographically random data
- Development fallback values MUST include a warning comment

### SEC-2 (MUST): Password Hashing

- Password hashing MUST use Argon2id (reinhardt-auth default)
- **NEVER** store passwords in plaintext
- **NEVER** implement custom password hashing

---

## ORM Best Practices

### ORM-1 (SHOULD): Use Field Attributes

- Use `#[field(...)]` attributes to define constraints:
  - `auto_now_add = true` for creation timestamps
  - `auto_now = true` for update timestamps
  - `max_length = N` for string length limits

### ORM-2 (SHOULD): Transaction Usage

- Use `transaction()` closure API for multi-step database operations
- Ensure all operations within a transaction are related and atomic

---

## Quick Reference

### MUST DO
- Generate migrations via `cargo make makemigrations`
- Keep module files (`views.rs`, `tests.rs`, etc.) as aggregator-only
- Create new apps via `reinhardt-admin startapp`
- Organize tests by type (`unit/`, `e2e/`, `integration/`)
- Name test files after the specific feature being tested
- Use ORM for all database operations
- One endpoint per file in `views/`
- Load secrets from environment variables
- Define URL patterns per application

### NEVER DO
- Manually edit migration files
- Place logic in aggregator module files
- Use generic test file names (`e2e_tests.rs`, `unit_test.rs`)
- Write raw SQL queries
- Hardcode secrets in source code
- Store passwords in plaintext

---

## Related Documentation

- **Root Standards**: ../CLAUDE.md
- **Testing Standards**: ../instructions/TESTING_STANDARDS.md
- **Module System**: ../instructions/MODULE_SYSTEM.md
- **Anti-Patterns**: ../instructions/ANTI_PATTERNS.md
