//! A person's initials in a circle.
//!
//! `reinhardt-pages` checked: `ui`, `tables`, and `form!` have no avatar. The
//! picture GitHub reports for a User is deliberately not shown: the page
//! content security policy allows images only from this origin and `data:`
//! (`img-src 'self' data:`), so GitHub's avatar host is unreachable, and the
//! approved design draws initials as well.

use reinhardt::pages::component::Page;
use reinhardt::pages::{ClassList, page, style_def};

#[style_def]
pub static AVATAR_STYLES: AvatarStyles = style! {
	globals {
		avatar_size: Length;
		avatar_size_lg: Length;
		surface_sunken: Color;
		border_default: Color;
		border_width: Length;
		radius_full: Length;
		ink_muted: Color;
		text_xs: Length;
		text_lg: Length;
		weight_semibold: Number;
	}
	.avatar {
		flex: none;
		display: grid;
		align-items: center;
		justify-items: center;
		width: globals.avatar_size;
		height: globals.avatar_size;
		background: globals.surface_sunken;
		border: (globals.border_width, solid, globals.border_default);
		border-radius: globals.radius_full;
		color: globals.ink_muted;
		font-size: globals.text_xs;
		font-weight: globals.weight_semibold;
	}
	.large {
		width: globals.avatar_size_lg;
		height: globals.avatar_size_lg;
		font-size: globals.text_lg;
	}
};

/// Size of an [`avatar`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AvatarSize {
	/// The sidebar size.
	#[default]
	Regular,
	/// The profile size.
	Large,
}

/// Returns the class list of an avatar of `size`.
pub fn avatar_classes(size: AvatarSize) -> ClassList {
	match size {
		AvatarSize::Regular => AVATAR_STYLES.avatar() + "",
		AvatarSize::Large => AVATAR_STYLES.avatar() + AVATAR_STYLES.large(),
	}
}

/// The initials of a person: the first letter of the first two words of
/// `display_name`, or the first letter of `login` when the name has none.
///
/// Letters are upper-cased. An empty name and login yield an empty string.
pub fn initials(display_name: &str, login: &str) -> String {
	let from_name: String = display_name
		.split_whitespace()
		.filter_map(|word| word.chars().next())
		.take(2)
		.flat_map(char::to_uppercase)
		.collect();
	if !from_name.is_empty() {
		return from_name;
	}
	login
		.chars()
		.next()
		.into_iter()
		.flat_map(char::to_uppercase)
		.collect()
}

/// Renders an avatar showing `initials`.
///
/// The circle is decorative: the person's name always sits next to it, so it is
/// hidden from assistive technology.
pub fn avatar(initials: String, size: AvatarSize) -> Page {
	let classes = avatar_classes(size);
	page!({
		span {
			class: classes,
			aria_hidden: "true",
			{ initials }
		}
	})
}
