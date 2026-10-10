//! Status badge and plain chip.
//!
//! `reinhardt-pages` checked: `ui` has only the action/resource primitives and
//! `tables` has `BooleanColumn` / `ChoiceColumn` cell renderers; none draws a
//! status marker with a label, so this component is custom.
//!
//! Each status pairs a color with its own marker shape and a text label, so a
//! status is never conveyed by color alone.

use reinhardt::pages::component::Page;
use reinhardt::pages::{ClassList, TranslatedText, page, style_def};

#[style_def]
pub static BADGE_STYLES: BadgeStyles = style! {
	globals {
		space_1: Length;
		space_2: Length;
		border_default: Color;
		border_width: Length;
		ink_muted: Color;
		radius_xs: Length;
		radius_sm: Length;
		radius_full: Length;
		text_sm: Length;
		leading_compact: Number;
		weight_medium: Number;
		badge_gap: Length;
		badge_padding_block: Length;
		badge_padding_inline_end: Length;
		badge_marker_size: Length;
		badge_marker_ring: Length;
		badge_marker_outline: Length;
		status_success_fg: Color;
		status_success_bg: Color;
		status_progress_fg: Color;
		status_progress_bg: Color;
		status_warning_fg: Color;
		status_warning_bg: Color;
		status_danger_fg: Color;
		status_danger_bg: Color;
		status_neutral_fg: Color;
		status_neutral_bg: Color;
	}
	.badge {
		display: inline-flex;
		align-items: center;
		gap: globals.badge_gap;
		padding: (globals.badge_padding_block, globals.badge_padding_inline_end, globals.badge_padding_block, globals.space_2);
		border-radius: globals.radius_full;
		font-size: globals.text_sm;
		font-weight: globals.weight_medium;
		line-height: globals.leading_compact;
		white-space: nowrap;
		&::before {
			content: "";
			flex: none;
			width: globals.badge_marker_size;
			height: globals.badge_marker_size;
			background: currentColor;
			border-radius: globals.radius_full;
		}
	}
	.success {
		background: globals.status_success_bg;
		color: globals.status_success_fg;
	}
	/* The spinning ring is animated by the `rc-spin` utility. */
	.progress {
		background: globals.status_progress_bg;
		color: globals.status_progress_fg;
		&::before {
			background: transparent;
			border: (globals.badge_marker_ring, solid, currentColor);
			border-top-color: transparent;
		}
	}
	/* A triangle drawn with borders, so the shape differs from the circle. */
	.warning {
		background: globals.status_warning_bg;
		color: globals.status_warning_fg;
		&::before {
			width: 0;
			height: 0;
			background: transparent;
			border-left: (globals.space_1, solid, transparent);
			border-right: (globals.space_1, solid, transparent);
			border-bottom: (globals.badge_marker_size, solid, currentColor);
			border-radius: 0;
		}
	}
	.danger {
		background: globals.status_danger_bg;
		color: globals.status_danger_fg;
		&::before {
			border-radius: globals.radius_xs;
		}
	}
	.neutral {
		background: globals.status_neutral_bg;
		color: globals.status_neutral_fg;
		&::before {
			background: transparent;
			border: (globals.badge_marker_outline, solid, currentColor);
		}
	}
	/* A plain label for facts that are not states: roles, sources. */
	.chip {
		display: inline-block;
		padding: (0, globals.space_2);
		border: (globals.border_width, solid, globals.border_default);
		border-radius: globals.radius_sm;
		color: globals.ink_muted;
		font-size: globals.text_sm;
		line-height: globals.leading_compact;
		white-space: nowrap;
	}
};

/// State a [`badge`] reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeStatus {
	/// Healthy or completed (circle marker).
	Success,
	/// Work in progress (spinning ring marker).
	Progress,
	/// Needs attention (triangle marker).
	Warning,
	/// Failed (square marker).
	Danger,
	/// No state to report (hollow circle marker).
	Neutral,
}

/// Returns the class list for a badge of the given status.
pub fn badge_classes(status: BadgeStatus) -> ClassList {
	let base = BADGE_STYLES.badge() + "";
	match status {
		BadgeStatus::Success => base + BADGE_STYLES.success(),
		BadgeStatus::Progress => base + BADGE_STYLES.progress() + "rc-spin",
		BadgeStatus::Warning => base + BADGE_STYLES.warning(),
		BadgeStatus::Danger => base + BADGE_STYLES.danger(),
		BadgeStatus::Neutral => base + BADGE_STYLES.neutral(),
	}
}

/// Renders a status badge with its text label.
pub fn badge(status: BadgeStatus, label: TranslatedText) -> Page {
	let classes = badge_classes(status);
	page!({
		span {
			class: classes,
			{ label }
		}
	})
}

/// Renders a plain chip for facts that are not states.
pub fn chip(label: TranslatedText) -> Page {
	page!({
		span {
			class: BADGE_STYLES.chip(),
			{ label }
		}
	})
}
