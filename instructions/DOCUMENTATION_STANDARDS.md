# Documentation Standards

## Purpose

This document defines documentation maintenance standards for the Reinhardt Cloud project, ensuring documentation stays synchronized with code changes.

---

## Core Principles

### DM-1 (MUST): Documentation Updates with Code Changes

**ALWAYS** update relevant documentation when implementing or modifying features.

**Documentation updates MUST be done in the same workflow as the code changes.**

**DO NOT** leave documentation outdated after code modifications.

**Example Workflow:**
```
1. Implement feature
2. Update relevant docs in the SAME session
3. Verify docs match implementation
4. Submit both code and docs together
```

The following diagram summarizes the documentation update workflow:

```mermaid
flowchart TD
    A[Code change made] --> B[Update lib.rs docs]
    A --> C[Update crate README if exists]
    A --> D[Update docs/ if relevant]
    A --> E[Update root README if major change]
    B & C & D & E --> F[Run doc tests: cargo test --doc]
    F --> G[Build docs: cargo doc --no-deps]
    G --> H[Verify examples work]
    H --> I[Check links are valid]
    I --> J[Submit code + docs together]
```

---

## Documentation Locations

### DM-2 (MUST): Documentation Locations

When modifying features, check and update the following documentation as applicable:

#### Project-Level Documentation
**File:** `README.md`
**Contents:**
- Project overview
- Installation instructions
- Quick start guide
- Main features (implemented only)
- Basic usage examples
- Links to detailed documentation

**When to Update:**
- Adding new major features
- Changing installation process
- Modifying project structure
- Updating dependencies

#### Crate-Level Documentation
**File:** `crates/<crate>/README.md` (if exists)
**Contents:**
- Crate-specific overview
- Crate features
- Usage examples
- API highlights

**File:** `crates/<crate>/src/lib.rs`
**Contents:**
- Module documentation (`//!`)
- Planned features section
- Architecture overview
- Code examples

#### Detailed Guides
**Location:** `docs/` directory
**Files:**
- `docs/GETTING_STARTED.md` - Getting started guide
- `docs/OPERATOR_GUIDE.md` - Operator deployment and configuration guide
- `docs/CRD_REFERENCE.md` - CRD API reference
- `docs/tutorials/` - Tutorial files

**When to Update:**
- Adding new features requiring detailed explanation
- Changing established patterns
- Adding new standards or conventions
- Updating tutorials

The following diagram illustrates where different types of documentation should be placed:

```mermaid
flowchart TD
    A[Documentation to write] --> B{What level?}
    B -->|"API reference"| C["lib.rs / module docs<br/>(inline rustdoc)"]
    B -->|"Crate overview"| D["crate README.md"]
    B -->|"Cross-crate guides"| E["docs/ directory"]
    B -->|"Project overview"| F["Root README.md"]
    B -->|"Internal standards"| G["instructions/ directory"]
    B -->|"Planned features"| H["lib.rs header<br/>(NOT README.md)"]
```

---

## Documentation Consistency

### DM-3 (MUST): Documentation Consistency

Ensure consistency across all documentation levels (project, crate, docs/).

**Consistency Checklist:**
- [ ] Terminology is consistent across all docs
- [ ] Code examples use the same style
- [ ] Version numbers match
- [ ] Links are valid and point to correct locations
- [ ] Examples actually work with current code
- [ ] API signatures match implementation

---

## Documentation Scope

### DM-4 (SHOULD): Documentation Scope

Update documentation for new features, modified features, deprecated features, and removed features. See examples in the workflow section for specific patterns.

---

## Documentation Quality

### DM-5 (MUST): Documentation Quality

Ensure high-quality documentation:

#### Examples Must Work
All code examples in documentation must be tested and working.

**Use Doc Tests:**
```rust
/// Reconciles the state of a ReinhardtApp.
///
/// # Examples
///
/// ```rust,no_run
/// use reinhardt_cloud_operator::reconcile;
///
/// // reconcile is called automatically by the controller
/// ```
pub async fn reconcile(obj: Arc<ReinhardtApp>, ctx: Arc<Context>) -> Result<Action> {
    // ...
}
```

**Test Documentation:**
```bash
cargo test --doc  # Runs all doc tests
```

---

## Planned Features Location

### DM-6 (MUST): Planned Features Location

**Planned Features MUST be documented in the crate's `lib.rs` file header.**

**DO NOT include Planned Features sections in README.md files.**

**Format in `lib.rs`:**
```rust
//! # reinhardt-cloud-operator
//!
//! Kubernetes operator for deploying Reinhardt web applications.
//!
//! ## Features
//!
//! - ReinhardtApp CRD for declarative app deployment ✅
//! - Automatic Deployment and Service creation ✅
//!
//! ## Planned Features
//!
//! - KEDA integration for autoscaling
//! - Multi-cluster federation support
//!
//! ## Examples
//!
//! ```rust,no_run
//! // Example code
//! ```
```

**Why?**
- Keeps planned features close to implementation code
- Better visibility during development
- README focuses on what's available NOW
- Reduces user confusion about what's actually implemented

---

## Rustdoc Formatting Standards

### DM-7 (MUST): Rustdoc Formatting Standards

Doc comments (`///` and `//!`) are processed by rustdoc and must follow specific formatting rules to avoid warnings and ensure proper HTML generation.

#### RD-1: Generic Types Must Be Wrapped in Backticks

Generic types like `<T>` are interpreted as HTML tags by rustdoc. Always wrap them in backticks.

```rust
// ✅ CORRECT
/// Returns `Option<String>` for the result
/// Uses `Result<T, Error>` for fallible operations

// ❌ INCORRECT (causes "unclosed HTML tag" warnings)
/// Returns Option<String> for the result
/// Uses Result<T, Error> for fallible operations
```

**Common types requiring backticks:**
- `Option<T>`, `Result<T, E>`, `Vec<T>`, `Box<T>`
- `Arc<T>`, `Rc<T>`, `RefCell<T>`, `Mutex<T>`
- `HashMap<K, V>`, `HashSet<T>`, `BTreeMap<K, V>`
- `Pin<T>`, `Future<Output = T>`, `Stream<Item = T>`

#### RD-2: Macro Attributes Must Be Wrapped in Backticks

Attributes like `#[derive]` are interpreted as markdown links by rustdoc. Always wrap them in backticks.

```rust
// ✅ CORRECT
/// Apply `#[derive(CustomResource)]` to define CRD types
/// Use `#[tokio::test]` for async test functions

// ❌ INCORRECT (causes "unresolved link" warnings)
/// Apply #[derive(CustomResource)] to define CRD types
```

#### RD-3: URLs Must Be Wrapped in Angle Brackets or Backticks

```rust
// ✅ CORRECT
/// See <https://github.com/kent8192/reinhardt-cloud> for source

// ❌ INCORRECT (causes "bare URL" warnings)
/// See https://github.com/kent8192/reinhardt-cloud for source
```

#### RD-4: Code Blocks Must Specify Language

````rust
// ✅ CORRECT
/// ```rust
/// let x = 42;
/// ```
///
/// ```yaml
/// apiVersion: paas.reinhardt-cloud.dev/v1alpha1
/// ```

// ❌ INCORRECT (may cause warnings)
/// ```
/// let x = 42;
/// ```
````

#### RD-5: Bracket Patterns Must Be Wrapped in Backticks

```rust
// ✅ CORRECT
/// Access the first replica via `replicas[0]`

// ❌ INCORRECT (causes "unresolved link" warnings)
/// Access the first replica via replicas[0]
```

#### RD-6: Feature-Gated Items Must Use Backticks (Not Intra-Doc Links)

```rust
// ✅ CORRECT (works regardless of enabled features)
/// Enable `keda` feature to use `ScaledObjectController`

// ❌ INCORRECT (causes "unresolved link" warnings when feature disabled)
/// Enable `keda` feature to use [`ScaledObjectController`]
```

#### Quick Reference Table

| Pattern | Incorrect | Correct |
|---------|-----------|---------|
| Generic types | `Option<T>` | `` `Option<T>` `` |
| Attributes | `#[derive]` | `` `#[derive]` `` |
| URLs | `https://...` | `<https://...>` or `` `https://...` `` |
| Code blocks | ` ``` ` | ` ```rust ` |
| Array access | `arr[0]` | `` `arr[0]` `` |
| Feature-gated items | `` [`TypeName`] `` | `` `TypeName` `` |

#### Verification

```bash
cargo doc --workspace --all-features 2>&1 | grep "warning:"
```

All doc comments should produce zero warnings.

---

## Diagram Standards

### DM-8 (SHOULD): Use Mermaid for Architecture Diagrams

When documenting architecture, data flow, or relationships between components,
**prefer Mermaid diagrams over ASCII art**.

#### Setup

Add `aquamarine` as a dependency in the crate's `Cargo.toml`:

```toml
[dependencies]
aquamarine = { workspace = true }
```

#### Usage

```rust
#[cfg_attr(doc, aquamarine::aquamarine)]
/// Reconciler state machine:
///
/// ```mermaid
/// stateDiagram-v2
///     [*] --> Pending: CRD created
///     Pending --> Reconciling: controller picks up
///     Reconciling --> Ready: all resources created
///     Reconciling --> Failed: error occurred
///     Failed --> Reconciling: requeued after backoff
/// ```
pub struct AppReconciler { }
```

#### When to Keep ASCII Art

- Simple inline diagrams (1-2 lines)
- Terminal output examples
- Code structure illustrations where text alignment matters

---

## Documentation Workflow

### Standard Documentation Update Process

```
1. ✅ Implement the code
2. ✅ Update lib.rs documentation
3. ✅ Update README.md if needed
4. ✅ Update crate README if exists
5. ✅ Update docs/ files if relevant
6. ✅ Run doc tests: cargo test --doc
7. ✅ Build docs: cargo doc --no-deps --open
8. ✅ Verify examples work
9. ✅ Check links are valid
10. ✅ Submit code + docs together
```

### Documentation Review Checklist

Before submitting:

- [ ] All relevant documentation files updated
- [ ] Code examples tested and working
- [ ] API signatures match implementation
- [ ] Terminology consistent across all docs
- [ ] Links are valid
- [ ] Formatting is correct
- [ ] No outdated information
- [ ] Planned features in lib.rs, not README
- [ ] Migration guides for breaking changes
- [ ] Doc tests pass
- [ ] Rustdoc warnings: zero (see DM-7)

---

## Related Documentation

- **Main Quick Reference**: @CLAUDE.md (see Quick Reference section)
- **Main standards**: @CLAUDE.md
- **Module system**: @instructions/MODULE_SYSTEM.md
- **Testing standards**: @instructions/TESTING_STANDARDS.md
- **Anti-patterns**: @instructions/ANTI_PATTERNS.md
