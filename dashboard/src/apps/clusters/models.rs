//! ORM models for clusters app.

pub mod cluster;

#[cfg(native)]
pub use cluster::Cluster;
