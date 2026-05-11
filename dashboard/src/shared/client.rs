//! Cross-app SPA building blocks shared by every dashboard app.
//!
//! Each app owns its own pages and components under
//! `dashboard/src/apps/<app>/client/`. Modules here are deliberately
//! cross-cutting — the global toast container, the deployment-status
//! badge, the WebSocket bootstrap, the SPA `AppState`, and the
//! framework's 404 page — and are surfaced through `crate::shared::client`
//! so the cut between "app-owned" and "shared" code is visible from a
//! single import path. See issue #578 for the broader migration plan.

pub mod components;
pub mod pages;
pub mod state;
pub mod ws;
