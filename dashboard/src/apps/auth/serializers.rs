//! Request/response serializers for auth endpoints.

pub mod change_password;
pub mod forgot_password;
pub mod login;
pub mod profile;
pub mod register;
pub mod reset_password;
pub mod verify_email;

pub use change_password::ChangePasswordRequest;
pub use forgot_password::ForgotPasswordRequest;
pub use login::LoginRequest;
pub use profile::{ProfileResponse, UpdateProfileRequest};
pub use register::RegisterRequest;
pub use reset_password::ResetPasswordRequest;
pub use verify_email::VerifyEmailPath;
