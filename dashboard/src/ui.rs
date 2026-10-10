//! Design system for the Control Plane.
//!
//! Reusable, target-neutral components built from the approved design
//! prototype in `docs/design/control-plane-prototype/`. Visual values come from
//! the tokens in `static/css/tokens.css`; each component owns a scoped
//! `#[style_def]` stylesheet that reads those tokens through `globals`.
//! Declarations the style DSL cannot express live in `static/css/utilities.css`
//! and are composed through the `rc-` classes named in each component.
//!
//! This module is shared by every application, so it lives at the library
//! level instead of inside one application's `client/components/`.

pub mod alert;
pub mod badge;
pub mod browser;
pub mod button;
pub mod code_block;
pub mod dialog;
pub mod empty_state;
pub mod field;
pub mod layout;
pub mod table;
pub mod theme;
