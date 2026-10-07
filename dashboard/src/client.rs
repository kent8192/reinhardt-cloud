//! Shared Pages surfaces and browser startup.

pub mod components;
#[cfg(client)]
pub mod lib;

pub mod navigation;
pub mod screens;

#[cfg(server)]
pub mod document;
