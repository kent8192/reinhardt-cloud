//! Request/response serializers for auth endpoints.

pub mod login;
pub mod register;

pub use login::LoginRequest;
pub use register::RegisterRequest;
