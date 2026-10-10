//! Configuration module for cloud_control_plane.

#[cfg(server)]
pub mod admin;
pub mod apps;
#[cfg(server)]
pub mod commands;
#[cfg(server)]
pub mod middleware;
#[cfg(server)]
pub mod settings;
#[cfg(all(server, feature = "commands-shell"))]
pub mod shell;
pub mod urls;
#[cfg(server)]
pub mod wasm;
#[cfg(server)]
pub mod web;
