//! Request/response serializers for auth endpoints.

pub mod login;
pub mod register;
pub mod token;

pub use login::LoginRequest;
pub use register::RegisterRequest;
pub use token::TokenResponse;
