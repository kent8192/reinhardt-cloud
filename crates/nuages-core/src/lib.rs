//! Core business logic for the nuages PaaS platform.
//!
//! This crate provides framework-agnostic domain services, error types,
//! and authentication utilities. It has no dependency on reinhardt or
//! any web framework.

pub mod auth;
pub mod error;
pub mod pagination;
pub mod services;

pub use error::ApiError;
pub use pagination::{PaginatedResponse, PaginationParams};
