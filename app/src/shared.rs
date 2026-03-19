//! Shared types and errors for WASM client and server communication.

pub mod errors;
pub mod types;

pub use errors::{AppError, FieldError};
pub use types::{AuthResponse, UserInfo};
