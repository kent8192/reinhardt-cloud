//! Request/response serializers for auth endpoints.

#[cfg(server)]
pub mod change_password;
#[cfg(server)]
pub mod forgot_password;
pub mod login;
#[cfg(server)]
pub mod profile;
pub mod register;
#[cfg(server)]
pub mod reset_password;

#[cfg(server)]
pub use change_password::ChangePasswordRequest;
#[cfg(server)]
pub use forgot_password::ForgotPasswordRequest;
pub use login::LoginRequest;
#[cfg(server)]
pub use profile::{ProfileResponse, UpdateProfileRequest};
pub use register::RegisterRequest;
#[cfg(server)]
pub use reset_password::ResetPasswordRequest;
