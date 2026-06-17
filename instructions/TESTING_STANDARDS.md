# Testing Standards

## Purpose

This document defines comprehensive testing standards for the Reinhardt Cloud project, ensuring high-quality, maintainable test coverage.

---

## Testing Philosophy

### TP-1 (MUST): Test Completeness

**NO skeleton implementations** - All tests MUST contain meaningful assertions.

**Definition of Skeleton Test:**
- A test that always passes (e.g., empty test, `assert!(true)`)
- Tests without any assertions
- Tests that don't actually verify behavior

**Requirements:**
- Tests MUST be capable of failing when the code is incorrect
- Documentation tests must be performed for all features you implement
- Do not implement test cases that are identical to documentation tests as unit tests or integration tests

**Examples:**

❌ **BAD - Skeleton Tests:**
```rust
#[test]
fn test_reconciler_creation() {
    // Empty test - always passes
}

#[test]
fn test_crd_validation() {
    let result = validate_spec(&spec);
    // No assertion - useless test
}
```

✅ **GOOD - Meaningful Tests:**
```rust
#[rstest]
fn test_reconciler_creation() {
    let reconciler = AppReconciler::new(context.clone());
    assert_eq!(reconciler.name(), "app-reconciler");
}

#[rstest]
fn test_crd_validation() {
    assert!(validate_spec(&valid_spec).is_ok());
    assert!(validate_spec(&invalid_spec).is_err());
}
```

### TP-2 (MUST): Reinhardt Cloud Component Usage

**EVERY** test case MUST use at least one component from the Reinhardt Cloud crate.

**Reinhardt Cloud Components Include:**
- Functions, variables, methods
- Structs, traits, enums
- Commands, macros
- All components present within the Reinhardt Cloud crate

**Why?** This ensures tests actually verify Reinhardt Cloud functionality rather than testing third-party libraries or standard library behavior.

---

## Test Organization

### TO-1 (MUST): Unit vs Integration Tests

Clear separation based on the nature of what is being tested:

#### Unit Tests
**Definition:** Tests that verify the behavior of a **single component**

**Component:** A single function, method, struct, trait, enum, or closely related group of items that serve a unified purpose.

**Location:** Within the functional crate being tested

**Characteristics:**
- Tests a component in isolation
- Verifies the component's behavior and edge cases
- Does not test interactions between multiple components

**Structure:**
```
crates/reinhardt-cloud-operator/
├── src/
│   ├── lib.rs
│   ├── reconciler.rs
│   └── crd.rs
└── tests/
    └── unit_tests.rs

// Unit tests in the same file
// src/reconciler.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[rstest]
    fn test_reconciler_action_requeue() {
        let action = Action::requeue(Duration::from_secs(30));
        assert_eq!(action.requeue_after(), Some(Duration::from_secs(30)));
    }
}
```

#### Integration Tests
**Definition:** Tests that verify the **integration points** (interfaces) between **two or more components**

**Location:**
- **Cross-crate integration:** MUST be placed in the `tests` crate at repository root
- **Within-crate integration:** Can be placed in the functional crate

**Structure:**
```
tests/                    // Cross-crate integration tests
├── Cargo.toml
└── integration/
    └── tests/
        └── operator_integration.rs

crates/reinhardt-cloud-operator/
└── tests/
    └── integration_tests.rs  // Within-crate integration (if needed)
```

### TO-2 (SHOULD): Test File Organization

Organize test files to mirror the source structure:

```
crates/reinhardt-cloud-operator/
├── src/
│   ├── lib.rs
│   ├── controller.rs
│   └── crd.rs
└── tests/
    ├── controller_tests.rs
    └── crd_tests.rs
```

---

## Test Implementation

### TI-1 (SHOULD): TODO Comments

If tests cannot be fully implemented, leave a `// TODO:` comment explaining why.

**DELETE** the TODO comment when the test is implemented.

**Example:**
```rust
#[rstest]
fn test_keda_autoscaling() {
    // TODO: Implement after adding KEDA ScaledObject support
    todo!("Waiting for KEDA integration")
}
```

### TI-2 (MUST): Unimplemented Feature Notation

For unimplemented features, use one of the following:

#### Option 1: `todo!()` macro
Use for features that **WILL** be implemented later

```rust
fn reconcile_ingress(app: &Project) -> Result<Action> {
    todo!("Add Ingress reconciliation - planned for next sprint")
}
```

#### Option 2: `unimplemented!()` macro
Use for features that **WILL NOT** be implemented (intentionally omitted)

```rust
fn legacy_v1_endpoint() -> String {
    unimplemented!("v1 API is intentionally not supported")
}
```

#### Option 3: `// TODO:` comment
Use for planning without runtime panics

```rust
// TODO: Add support for custom ingress annotations
fn build_ingress(app: &Project) -> Ingress {
    // Temporary implementation
    Ingress::default()
}
```

**Macro Selection Guidelines:**
- `todo!()` → Features that WILL be implemented
- `unimplemented!()` → Features that WILL NOT be implemented
- `// TODO:` → Planning notes

**DELETE `todo!()` and `// TODO:` when implemented**
**KEEP `unimplemented!()` for permanently excluded features**

### TI-3 (MUST): Test Cleanup

**ALL** files, directories, or environmental changes created during tests **MUST** be deleted upon test completion.

**Techniques:**
- Test fixtures with `Drop` implementations
- `tempfile` crate for temporary files
- Explicit cleanup in test teardown

**Example:**
```rust
#[rstest]
fn test_kubeconfig_operations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("kubeconfig");

    // Test code that creates files
    std::fs::write(&config_path, "apiVersion: v1").unwrap();

    // Cleanup happens automatically when temp_dir drops
}
```

### TI-4 (MUST): Global State Management

Tests that modify global state MUST be serialized using the `serial_test` crate.

Use named serial groups: `#[serial(group_name)]` to serialize only related tests.

**Setup:**
```toml
# Cargo.toml
[dev-dependencies]
serial_test = { workspace = true }
```

**Example:**
```rust
use serial_test::serial;

#[rstest]
#[serial(kube_config)]
fn test_kubeconfig_override() {
    set_kubeconfig_path("/tmp/test-kubeconfig");
    assert_eq!(get_kubeconfig_path(), "/tmp/test-kubeconfig");
    reset_kubeconfig_path();  // ✅ Cleanup
}
```

### TI-5 (MUST): Assertion Strictness

**Use strict assertions with exact value comparisons instead of loose pattern matching.**

**Preferred Methods:**
- `assert_eq!(actual, expected)` - For exact value equality
- `assert_ne!(actual, unexpected)` - For exact value inequality
- `assert!(matches!(value, Pattern))` - For pattern matching with specific variants

**Avoid Loose Assertions:**
- ❌ `assert!(string.contains("substring"))` - Too permissive, may match unintended content
- ❌ `assert!(result.is_ok())` without checking the contained value
- ❌ `assert!(value > 0)` when you know the exact expected value

**Exception:**
Loose assertions are acceptable ONLY when strict assertions are impossible or impractical:
- Random values (e.g., UUIDs, resource versions)
- System-dependent values (e.g., timestamps, Kubernetes-assigned names)
- Non-deterministic operations

**Justification Requirement:**
When using loose assertions, add a comment explaining why strict assertions are not possible.

### TI-6 (SHOULD): Arrange-Act-Assert (AAA) Pattern

All tests SHOULD follow the **Arrange-Act-Assert (AAA)** pattern for clear, consistent structure.

**AAA Phases:**

| Phase | Purpose | BDD Equivalent |
|-------|---------|----------------|
| **Arrange** | Set up test preconditions and inputs | Given |
| **Act** | Execute the behavior under test | When |
| **Assert** | Verify the expected outcomes | Then |

**Comment Labels:**

Use ONLY these standard labels:
- `// Arrange` - Setup phase
- `// Act` - Execution phase
- `// Assert` - Verification phase

❌ **Non-standard labels are prohibited:** `// Setup`, `// Execute`, `// Verify`, `// Given`, `// When`, `// Then`

**Examples:**

```rust
#[rstest]
fn test_crd_spec_defaults() {
    // Arrange
    let spec = ProjectSpec {
        image: "myapp:latest".to_string(),
        replicas: None,
    };

    // Act
    let resolved = spec.with_defaults();

    // Assert
    assert_eq!(resolved.replicas, 1);
}
```

```rust
#[rstest]
fn test_reconciler_action(app_fixture: Arc<Project>) {
    // Arrange: provided by app_fixture

    // Act
    let action = compute_desired_action(&app_fixture);

    // Assert
    assert!(matches!(action, DesiredAction::CreateDeployment));
}
```

**Comment Omission:**

AAA comments MAY be omitted when the test body is **5 lines or fewer** and the phases are self-evident.

---

## Infrastructure Testing

### IT-1 (SHOULD): TestContainers for Infrastructure

Use **TestContainers** for tests requiring actual infrastructure:
- Databases (PostgreSQL, MySQL)
- Message queues (Redis, RabbitMQ)
- Cache systems

**Benefits:**
- Tests use real infrastructure, not mocks
- More confidence in production behavior

**Example:**
```rust
use testcontainers::{clients, images};

#[rstest]
#[tokio::test]
async fn test_database_integration(#[future] postgres_container: PostgresFixture) {
    // Arrange
    let (_container, pool) = postgres_container.await;

    // Act
    let result = pool.execute("SELECT 1").await;

    // Assert
    assert!(result.is_ok());
}
```

### IT-2 (MUST): Prevent Flaky Tests with TestContainers

When using TestContainers for parallel test execution, limit concurrency:

```toml
# .cargo/nextest.toml
[profile.default]
max-tests-per-run = 8
slow-timeout = "60s"
timeout = "120s"
retries = { backoff = "exponential", max-retries = 2, seed = 12345 }
```

---

## rstest Best Practices

### TF-0 (MUST): rstest for All Test Cases

**ALL** test cases in this project MUST use **rstest** as the test framework.

**Requirements:**
- Import `rstest::*` in all test modules
- Use `#[rstest]` attribute instead of `#[test]`
- Use `#[rstest]` with `#[tokio::test]` for async tests
- Leverage fixtures for setup/teardown

❌ **BAD - Using standard #[test]:**
```rust
#[test]
fn test_basic_operation() {
    let reconciler = AppReconciler::new();
    assert!(reconciler.is_ready());
}
```

✅ **GOOD - Using rstest:**
```rust
use rstest::*;

#[rstest]
fn test_basic_operation(reconciler_fixture: AppReconciler) {
    assert!(reconciler_fixture.is_ready());
}

#[rstest]
#[tokio::test]
async fn test_async_operation(#[future] postgres_container: PostgresFixture) {
    let (_container, pool) = postgres_container.await;
    assert!(pool.is_connected().await);
}
```

### TF-1 (SHOULD): rstest Fixture Pattern

Use **rstest** fixtures for reusable test setup and dependency injection.

Fixtures serve as the **Arrange** phase in the AAA pattern.

#### Basic Fixture

```rust
use rstest::*;

#[fixture]
fn app_spec() -> ProjectSpec {
    ProjectSpec {
        image: "myapp:latest".to_string(),
        replicas: Some(2),
    }
}

#[rstest]
fn test_with_fixture(app_spec: ProjectSpec) {
    assert_eq!(app_spec.replicas, Some(2));
}
```

#### Async Fixture

```rust
#[fixture]
async fn postgres_fixture() -> (ContainerAsync<GenericImage>, Pool<Postgres>) {
    // Setup PostgreSQL container and pool
    // ...
}

#[rstest]
#[tokio::test]
async fn test_with_async_fixture(
    #[future] postgres_fixture: (ContainerAsync<GenericImage>, Pool<Postgres>)
) {
    let (_container, pool) = postgres_fixture.await;
    // Test code
}
```

**IMPORTANT**: Always include `#[future]` for async fixtures, and `.await` them in the test body.

### TF-2 (SHOULD): TestContainers with rstest

Combine rstest fixtures with TestContainers for database testing:

```rust
#[fixture]
async fn postgres_container() -> (ContainerAsync<GenericImage>, Pool<Postgres>) {
    let container = GenericImage::new("postgres", "17-alpine")
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_HOST_AUTH_METHOD", "trust")
        .start()
        .await
        .expect("Failed to start PostgreSQL container");

    let host_port = container.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres@localhost:{}/postgres", host_port);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    (container, pool)
}

#[rstest]
#[tokio::test]
async fn test_database_operations(
    #[future] postgres_container: (ContainerAsync<GenericImage>, Pool<Postgres>)
) {
    let (_container, pool) = postgres_container.await;

    // Act
    let result = sqlx::query("SELECT 1").execute(&pool).await;

    // Assert
    assert!(result.is_ok());
}
```

---

## Quick Reference

### ✅ MUST DO
- Use `rstest` for ALL test cases (no plain `#[test]`)
- Every test MUST have at least one meaningful assertion
- Every test MUST use at least one Reinhardt Cloud component
- Follow Arrange-Act-Assert (AAA) pattern with `// Arrange`, `// Act`, `// Assert` comments
- Use strict assertions (`assert_eq!`) instead of loose matching
- Use `#[serial(group_name)]` for global state tests
- Clean up ALL test artifacts
- Use SeaQuery (not raw SQL) for SQL construction in tests

### ❌ NEVER DO
- Use plain `#[test]` instead of `#[rstest]`
- Create skeleton tests (tests without assertions)
- Use loose assertions without justification comment
- Use non-standard phase labels (`// Setup`, `// Execute`, `// Verify`)
- Write raw SQL strings in tests (use SeaQuery instead)
- Leave test artifacts uncleaned

---

## Related Documentation

- **Main Quick Reference**: @CLAUDE.md (see Quick Reference section)
- **Anti-Patterns**: @instructions/ANTI_PATTERNS.md
- **Module System**: @instructions/MODULE_SYSTEM.md
