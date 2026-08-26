//! Server-side auth routes that require browser navigation.

mod account;
pub(crate) mod oauth;

pub use account::{api_me, verify_email};
pub use oauth::{OAuthCallbackQuery, OAuthStartQuery, oauth_callback, oauth_start};
