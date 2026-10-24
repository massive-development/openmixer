//! The interface font: Inter (SIL OFL 1.1, see `resources/fonts/Inter-OFL.txt`) bundled into the
//! binary and registered with GPUI's text system at start-up, with tabular numerals switched on so
//! BPM, time and counters do not jitter as digits change.
//!
//! The theme names the family (`font.family`) and whether numerals are tabular
//! (`font.tabularNumerals`); a theme that names another installed family simply gets that one,
//! because the bundled face is registered under its own name, "Inter".

use std::borrow::Cow;
use std::sync::Arc;

use gpui::{font, App, Font, FontFeatures};
use om_ui::theme::ThemeTokens;

/// Inter Regular (static TrueType, includes the `tnum` feature).
static INTER_REGULAR: &[u8] = include_bytes!("../../../resources/fonts/Inter-Regular.ttf");

/// Registers the bundled font. A failure is logged and the system font is used instead.
pub fn register(cx: &App) {
    if let Err(err) = cx
        .text_system()
        .add_fonts(vec![Cow::Borrowed(INTER_REGULAR)])
    {
        eprintln!("openmixer: could not register the bundled font: {err}");
    }
}

/// The OpenType features a theme asks for: `tnum` (tabular numerals) when enabled.
pub fn features(theme: &ThemeTokens) -> FontFeatures {
    if theme.tabular_numerals {
        FontFeatures(Arc::new(vec![("tnum".to_owned(), 1)]))
    } else {
        FontFeatures::default()
    }
}

/// The font set on the window root; every text element inherits it.
pub fn interface_font(theme: &ThemeTokens) -> Font {
    Font {
        features: features(theme),
        ..font(theme.font_family.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_font_is_a_truetype_file_with_the_tnum_feature() {
        assert_eq!(&INTER_REGULAR[..4], &[0, 1, 0, 0], "TrueType magic");
        assert!(INTER_REGULAR.windows(4).any(|w| w == b"tnum"));
    }

    #[test]
    fn tabular_numerals_follow_the_theme() {
        let mut t = ThemeTokens::builtin_dark();
        assert_eq!(
            interface_font(&t).features.tag_value_list(),
            &[("tnum".to_owned(), 1)]
        );
        t.tabular_numerals = false;
        assert!(interface_font(&t).features.tag_value_list().is_empty());
        assert_eq!(interface_font(&t).family.as_ref(), "Inter");
    }
}
