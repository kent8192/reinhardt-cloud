//! Page layouts.

pub mod signed_in;
pub mod signed_out;

/// Logical path of the small logo mark under `static/`.
pub const LOGO_MARK_PATH: &str = "img/logo-mark-small.png";

/// Pixel size of [`LOGO_MARK_PATH`]; the image is never upscaled.
pub const LOGO_MARK_SIZE: (u32, u32) = (142, 98);

/// `id` of the main landmark, targeted by the skip link.
pub const MAIN_ID: &str = "main";
