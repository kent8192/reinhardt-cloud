//! Views module for accounts app (Pages)
//!
//! Define ViewSets here. Each `pub mod` declaration corresponds to a file
//! under the `views/` directory.
//!
//! For multi-file views that need re-exports for discovery, use:
//! ```rust,ignore
//! flatten_imports! {
//!     pub mod example;
//! }
//! ```
//!
//! # Example ViewSet
//!
//! ```rust,ignore
//! use reinhardt::prelude::*;
//! use reinhardt::viewset;
//!
//! // Import your model here
//! // use crate::apps::accounts::models::AccountsItem;
//!
//! #[viewset]
//! pub struct AccountsViewSet;
//! ```
