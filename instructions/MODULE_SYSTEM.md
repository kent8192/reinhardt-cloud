# Rust 2024 Edition Module System Standards

## Purpose

This document defines the module system standards for the Nuages project using Rust 2024 Edition conventions.

## Core Principle

**MUST USE `module.rs` + `module/` directory structure (Rust 2024 Edition)**

**NEVER USE `mod.rs` files** (Rust 2015/2018 deprecated pattern)

---

## Basic Patterns

The following diagram helps select the appropriate module pattern:

```mermaid
flowchart TD
    A[New module needed] --> B{Has submodules?}
    B -->|No| C["Pattern 1: Single file<br/>module.rs"]
    B -->|Yes| D{Has nested sub-submodules?}
    D -->|No| E["Pattern 2: Entry point + directory<br/>module.rs + module/"]
    D -->|Yes| F{Nesting depth > 4?}
    F -->|Yes| G[Refactor: reduce nesting]
    F -->|No| H["Pattern 3: Hierarchical<br/>module.rs + module/ with nested dirs"]
```

### Pattern 1: Small Module (Single File)

For modules with no submodules:

```
src/
├── lib.rs          // mod utils;
└── utils.rs        // pub fn helper() {}
```

### Pattern 2: Medium Module (With Submodules)

For modules with submodules:

```
src/
├── lib.rs          // mod controller;
├── controller.rs   // pub mod reconciler; pub mod watcher;
└── controller/
    ├── reconciler.rs
    └── watcher.rs
```

**Key Points:**
- `controller.rs` is the entry point
- Declare submodules in `controller.rs`: `pub mod reconciler; pub mod watcher;`
- Parent declares with: `mod controller;` in `lib.rs`

### Pattern 3: Large Module (Hierarchical Structure)

For complex modules with nested submodules:

```
src/
├── lib.rs              // mod api;
├── api.rs              // pub mod handlers; pub mod middleware;
└── api/
    ├── handlers.rs     // pub mod app; pub mod ingress;
    ├── handlers/
    │   ├── app.rs
    │   └── ingress.rs
    ├── middleware.rs
    └── middleware/
        └── auth.rs
```

**Key Points:**
- Each level has an entry point file (`api.rs`, `handlers.rs`, `middleware.rs`)
- Submodules declared in their parent's entry point
- Avoid nesting beyond 4 levels

---

## Visibility and Encapsulation

### Controlling Public API with `pub use`

Use `pub use` in module entry points to control what's exposed:

```rust
// controller.rs (entry point)
mod reconciler;     // Private submodule
mod watcher;        // Private submodule

// Public API - explicitly re-export
pub use reconciler::{Reconciler, ReconcilerConfig};
pub use watcher::Watcher;

// Internal implementation remains private
// reconciler::InternalState is not visible externally
```

**Benefits:**
- Clear separation between public API and implementation details
- Easy to refactor internal structure without breaking external code
- Explicit control over exported items

---

## Anti-Patterns (What NOT to Do)

For detailed anti-patterns and examples, see @instructions/ANTI_PATTERNS.md. Key module system anti-patterns:

- **Using `mod.rs`**: Use `module.rs` instead (Rust 2024 Edition)
- **Glob imports**: Use explicit `pub use` (except in test modules)
- **Circular dependencies**: Extract common types to break cycles
- **Excessive flat structure**: Group related files in module directories

---

## Filesystem Structure Principles

### 1. Single Entry Point
Each module has exactly one entry point file (`module.rs`), not `module/mod.rs`

### 2. Logical Hierarchy
File structure mirrors the logical module hierarchy

### 3. Explicit Publicity
Use `pub use` to intentionally expose API, don't default to everything public

### 4. Limited Depth
Avoid excessive nesting (>4 levels makes navigation difficult)

---

## Migration Guide

### Converting from `mod.rs` to `module.rs`

If you have old code using `mod.rs`:

**Before:**
```
src/controller/mod.rs
```

**After:**
```
src/controller.rs
```

**Steps:**
1. Move `module/mod.rs` → `module.rs`
2. Keep `mod submodule;` declarations in `module.rs`
3. Maintain `pub use` re-exports
4. No changes needed in parent module declaration (`mod module;` stays the same)

---

## Example: Complete Module Structure

Here's a complete example showing best practices:

```
src/
├── lib.rs
│   // mod controller;
│   // mod crd;
│
├── controller.rs
│   // pub mod app;
│   // pub mod ingress;
│   // pub use app::{AppController, AppContext};
│
├── controller/
│   ├── app.rs
│   │   // pub struct AppController { ... }
│   │   // pub struct AppContext { ... }
│   │   // struct InternalState { ... }  // Not re-exported
│   │
│   └── ingress.rs
│       // pub struct IngressController { ... }
│
├── crd.rs
│   // pub mod reinhardt_app;
│   // pub use reinhardt_app::ReinhardtApp;
│
└── crd/
    └── reinhardt_app.rs
        // #[derive(CustomResource)]
        // pub struct ReinhardtAppSpec { ... }
```

**Usage from external code:**
```rust
use nuages::controller::{AppController, AppContext};  // ✅ Works - explicitly re-exported
use nuages::crd::ReinhardtApp;                        // ✅ Works
use nuages::controller::app::InternalState;           // ❌ Error - not re-exported
```

---

## Related Documentation

- **Main Quick Reference**: @CLAUDE.md (see Quick Reference section)
- **Main standards**: @CLAUDE.md
- **Project structure**: @README.md
