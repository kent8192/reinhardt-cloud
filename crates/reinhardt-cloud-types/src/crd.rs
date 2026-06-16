//! CRD type definitions for the Reinhardt Cloud PaaS platform.
//!
//! Defines the `Project` custom resource following the Kubernetes
//! operator pattern with strongly typed spec and status fields.

pub mod auth;
pub mod cache;
pub mod database;
pub mod enums;
pub mod infrastructure;
pub mod isolation;
pub mod mail;
pub mod pages;
pub mod plugins;
pub mod policy;
pub mod service_account;
pub mod source;
pub mod spec;
pub mod status;
pub mod storage;
pub mod tenant;
pub mod worker;

pub use auth::*;
pub use cache::*;
pub use database::*;
pub use enums::*;
pub use infrastructure::*;
pub use isolation::*;
pub use mail::*;
pub use pages::*;
pub use plugins::*;
pub use policy::*;
pub use service_account::*;
pub use source::*;
pub use spec::*;
pub use status::*;
pub use storage::*;
pub use tenant::*;
pub use worker::*;
