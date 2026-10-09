//! Service modules for the github application.
//!
//! Client-only services live under `services/client/`; server-only services
//! live under `services/server/`.

#[cfg(client)]
pub mod client;

#[cfg(server)]
pub mod server;
