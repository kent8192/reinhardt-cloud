//! View functions for auth endpoints.

pub mod change_password;
pub mod forgot_password;
pub mod login;
pub mod profile;
pub mod register;
pub mod reset_password;
pub mod verify_email;

pub use change_password::change_password;
pub use forgot_password::forgot_password;
pub use login::login;
pub use profile::{profile, profile_update};
pub use register::register;
pub use reset_password::reset_password;
pub use verify_email::verify_email;
