//! The bundled fonts and the type tiers. Sansation Bold (SIL OFL 1.1), Source Sans 3 Regular and
//! Medium (SIL OFL 1.1) and Ubuntu Mono Regular and Bold (Ubuntu Font Licence 1.0) are compiled in and
//! registered with GPUI's text system at start-up; their licences sit beside them in
//! `resources/fonts`. GPUI picks the nearest weight among the static faces of one family, so
//! Source Sans 3's two files share one family name (docs/spikes/DECISIONS.md, "Faceplate P2").
//!
//! The theme names the three families (`font.display`, `font.body`, `font.mono`); a theme that
//! names another installed family gets that one. Every number goes through [`numeral`].

use std::borrow::Cow;
use std::sync::Arc;

use gpui::{
    div, font, px, relative, App, Div, Font, FontFeatures, FontWeight, ParentElement, SharedString,
    Styled,
};
use om_ui::theme::{FontSlot, ThemeTokens, TypeRole};

/// The bundled faces: (file name, bytes). Ubuntu Mono has no 500; the 500 numerals resolve to its
/// Regular, as a browser does with the comp's 400/700 web fonts. Its Bold sets the deck letter and
/// the state words (LOCK, CLIP).
pub static BUNDLED_FACES: [(&str, &[u8]); 5] = [
    (
        "Sansation-Bold.ttf",
        include_bytes!("../../../resources/fonts/Sansation-Bold.ttf"),
    ),
    (
        "SourceSans3-Regular.ttf",
        include_bytes!("../../../resources/fonts/SourceSans3-Regular.ttf"),
    ),
    (
        "SourceSans3-Medium.ttf",
        include_bytes!("../../../resources/fonts/SourceSans3-Medium.ttf"),
    ),
    (
        "UbuntuMono-Regular.ttf",
        include_bytes!("../../../resources/fonts/UbuntuMono-Regular.ttf"),
    ),
    (
        "UbuntuMono-Bold.ttf",
        include_bytes!("../../../resources/fonts/UbuntuMono-Bold.ttf"),
    ),
];

/// The bundled faces as GPUI takes them.
pub fn bundled_faces() -> Vec<Cow<'static, [u8]>> {
    BUNDLED_FACES
        .iter()
        .map(|(_, bytes)| Cow::Borrowed(*bytes))
        .collect()
}

/// Registers the bundled fonts. A failure is logged and the system font is used instead.
pub fn register(cx: &App) {
    if let Err(err) = cx.text_system().add_fonts(bundled_faces()) {
        eprintln!("openmixer: could not register the bundled fonts: {err}");
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

/// The family a slot is set in, as GPUI names it.
pub fn family(theme: &ThemeTokens, slot: FontSlot) -> SharedString {
    theme.family(slot).to_owned().into()
}

/// The font set on the window root (the body family); every text element inherits it.
pub fn interface_font(theme: &ThemeTokens) -> Font {
    Font {
        features: features(theme),
        ..font(family(theme, FontSlot::Body))
    }
}

/// Sets an element's text to one tier of the type scale: family, size, weight and line height.
pub trait TypeExt: Styled + Sized {
    fn type_role(self, t: &ThemeTokens, role: TypeRole) -> Self {
        self.font_family(family(t, role.slot()))
            .text_size(px(t.type_size(role)))
            .font_weight(FontWeight(f32::from(role.weight())))
            .line_height(relative(role.line_height()))
    }
}

impl<E: Styled> TypeExt for E {}

/// The size of a drawn number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Numeral {
    /// Values: dB, gain, counters, table columns (13 px).
    Value,
    /// The primary readouts: BPM, key, time (21 px).
    Readout,
    /// Fader dB scale ticks only (10 px).
    Scale,
}

impl Numeral {
    pub fn role(self) -> TypeRole {
        match self {
            Numeral::Value => TypeRole::Numeral,
            Numeral::Readout => TypeRole::NumeralLarge,
            Numeral::Scale => TypeRole::Scale,
        }
    }
}

/// A number as text, always in the mono family so values do not shift width as they change (the
/// Mono Numbers Rule). Colour and alignment are the caller's.
pub fn numeral(t: &ThemeTokens, size: Numeral, text: impl Into<SharedString>) -> Div {
    div().type_role(t, size.role()).child(text.into())
}

/// The size of a [`mark`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkSize {
    /// A state word beside a control (LOCK, CLIP): the label size.
    Word,
    /// The deck letter in its tag: the numeral size.
    Letter,
}

/// A deck letter or a state word: mono bold, line height 1 (canvas section 5, "Sync locked" and
/// "Deck identity"). Colour is the caller's.
pub fn mark(t: &ThemeTokens, size: MarkSize, text: impl Into<SharedString>) -> Div {
    let px_size = match size {
        MarkSize::Word => t.type_label,
        MarkSize::Letter => t.type_numeral,
    };
    div()
        .font_family(family(t, FontSlot::Mono))
        .text_size(px(px_size))
        .font_weight(FontWeight::BOLD)
        .line_height(relative(1.0))
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_bundled_face_is_a_truetype_file() {
        for (name, bytes) in &BUNDLED_FACES {
            assert_eq!(&bytes[..4], &[0, 1, 0, 0], "{name}: TrueType magic");
        }
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
        assert_eq!(interface_font(&t).family.as_ref(), "Source Sans 3");
    }

    #[test]
    fn numbers_are_set_in_the_mono_family() {
        let t = ThemeTokens::builtin_dark();
        for size in [Numeral::Value, Numeral::Readout, Numeral::Scale] {
            assert_eq!(size.role().slot(), FontSlot::Mono);
        }
        assert_eq!(
            family(&t, Numeral::Readout.role().slot()).as_ref(),
            "Ubuntu Mono"
        );
        assert_eq!(t.type_size(Numeral::Readout.role()), 21.0);
    }
}
