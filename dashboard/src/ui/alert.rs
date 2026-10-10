//! Inline alert for outcomes a person has to act on.

use reinhardt::pages::component::Page;
use reinhardt::pages::{ClassList, TranslatedText, page, style_def};

#[style_def]
pub static ALERT_STYLES: AlertStyles = style! {
	globals {
		space_3: Length;
		space_4: Length;
		radius_md: Length;
		border_default: Color;
		surface_raised: Color;
		ink_strong: Color;
		ink_muted: Color;
		measure_prose: Length;
		leading_prose: Number;
		weight_semibold: Number;
		status_warning_fg: Color;
		status_danger_fg: Color;
	}
	.alert {
		padding: (globals.space_3, globals.space_4);
		border: (1px, solid, globals.border_default);
		border-left-width: 3px;
		border-radius: globals.radius_md;
		background: globals.surface_raised;
	}
	.warning {
		border-left-color: globals.status_warning_fg;
	}
	.danger {
		border-left-color: globals.status_danger_fg;
	}
	.title {
		color: globals.ink_strong;
		font-weight: globals.weight_semibold;
	}
	.body {
		max-width: globals.measure_prose;
		line-height: globals.leading_prose;
		color: globals.ink_muted;
	}
};

/// Severity of an [`alert`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertTone {
	/// Neutral information.
	#[default]
	Info,
	/// Something is not as expected but nothing failed.
	Warning,
	/// An operation failed.
	Danger,
}

/// Returns the class list for an alert of the given tone.
pub fn alert_classes(tone: AlertTone) -> ClassList {
	let base = ALERT_STYLES.alert() + "";
	match tone {
		AlertTone::Info => base,
		AlertTone::Warning => base + ALERT_STYLES.warning(),
		AlertTone::Danger => base + ALERT_STYLES.danger(),
	}
}

/// Renders an alert announced to assistive technology (`role="alert"`).
pub fn alert(tone: AlertTone, title: TranslatedText, body: TranslatedText) -> Page {
	let classes = alert_classes(tone);
	page!({
		div {
			class: classes,
			role: "alert",
			p {
				class: ALERT_STYLES.title(),
				{ title }
			}
			p {
				class: ALERT_STYLES.body(),
				{ body }
			}
		}
	})
}
