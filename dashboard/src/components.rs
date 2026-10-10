//! Shared components for the Control Plane.
//!
//! The design from `docs/design/control-plane-prototype/` is applied to the
//! primitives that `reinhardt-pages` already ships (`ui::ActionButton`,
//! `ui::FormActionButton`, `ui::ActionResultPanel`, `ui::ResourcePanel`,
//! `Portal`, and the `form!` / `ClientForm` rendering) through typed
//! `#[style_def]` stylesheets and the tokens in `static/css/tokens.css`. A
//! component is written here only where `reinhardt-pages` has no primitive;
//! each of those carries a comment naming the primitives that were checked.
//!
//! Placement rules:
//!
//! - Components shared by several applications live here, in `src/components/`.
//! - Route-backed pages (routing targets, `#[component]` and `#[layout]`
//!   functions) live in `src/apps/<app>/client/components/` of the owning
//!   application.
//!
//! Declarations the style DSL cannot express live in `static/css/utilities.css`
//! and are composed through the `rc-` classes named in each component.

pub mod alert;
pub mod badge;
pub mod browser;
pub mod button;
pub mod code_block;
pub mod dialog;
pub mod empty_state;
pub mod form_styles;
pub mod layout;
pub mod table_styles;
pub mod theme;
