//! Request/response serializers for auth endpoints.

#[cfg(native)]
pub mod change_password;
#[cfg(native)]
pub mod forgot_password;
pub mod login;
#[cfg(native)]
pub mod profile;
pub mod register;
#[cfg(native)]
pub mod reset_password;

#[cfg(native)]
pub use change_password::ChangePasswordRequest;
#[cfg(native)]
pub use forgot_password::ForgotPasswordRequest;
pub use login::LoginRequest;
#[cfg(native)]
pub use profile::{ProfileResponse, UpdateProfileRequest};
pub use register::RegisterRequest;
#[cfg(native)]
pub use reset_password::ResetPasswordRequest;
