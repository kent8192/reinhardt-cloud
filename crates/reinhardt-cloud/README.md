# reinhardt-cloud

Facade crate for Reinhardt Cloud library components.

Use this crate when an application or integration needs one dependency that exposes the public library crates under stable namespaces:

```rust
use reinhardt_cloud::{core, grpc, k8s, proto, telemetry, types};
```

The CLI, operator, and agent remain binary crates.
