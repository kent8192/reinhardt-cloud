//! Configuration module for cloud_control_plane.

pub mod apps;
#[cfg(server)]
pub mod settings;
#[cfg(all(server, feature = "commands-shell"))]
pub mod shell;
pub mod urls;
#[cfg(server)]
pub mod wasm;
