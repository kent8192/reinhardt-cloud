//! CRD type definitions for the Reinhardt Cloud PaaS platform.
//!
//! Defines the `ReinhardtApp` custom resource following the Kubernetes
//! operator pattern with strongly typed spec and status fields.

pub mod auth;
pub mod cache;
pub mod database;
pub mod enums;
pub mod mail;
pub mod policy;
pub mod spec;
pub mod status;
pub mod storage;
pub mod worker;

pub use auth::*;
pub use cache::*;
pub use database::*;
pub use enums::*;
pub use mail::*;
pub use policy::*;
pub use spec::*;
pub use status::*;
pub use storage::*;
pub use worker::*;
