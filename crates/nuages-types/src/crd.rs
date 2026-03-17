//! CRD type definitions for the Nuages PaaS platform.
//!
//! Defines the `ReinhardtApp` custom resource following the Kubernetes
//! operator pattern with strongly typed spec and status fields.

pub mod enums;
pub mod spec;
pub mod status;

pub use enums::*;
pub use spec::*;
pub use status::*;
