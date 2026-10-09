//! Models module for accounts app
//!
//! Replace this placeholder with the models for the app.
//!
//! Authentication `User` models need the `#[user]` macro and the auth field
//! set expected by your project. For the basics tutorial, copy the complete
//! `User` model from the tutorial chapter instead of adapting this placeholder.
//!
//! # Example
//!
//! ```rust,ignore
//! use reinhardt::prelude::*;
//! use reinhardt::{Deserialize, Serialize};
//!
//! #[model(app_label = "accounts", table_name = "accounts_item")]
//! #[derive(Serialize, Deserialize)]
//! pub struct AccountsItem {
//!     #[field(primary_key = true)]
//!     pub id: i64,
//!
//!     #[field(max_length = 255)]
//!     pub name: String,
//! }
//! ```
