//! Models module for health app
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
//! #[model(app_label = "health", table_name = "health_item")]
//! #[derive(Serialize, Deserialize)]
//! pub struct HealthItem {
//!     #[field(primary_key = true)]
//!     pub id: i64,
//!
//!     #[field(max_length = 255)]
//!     pub name: String,
//! }
//! ```
