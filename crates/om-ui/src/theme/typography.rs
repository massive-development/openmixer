//! The type tiers of `docs/design-system/DESIGN.md`: which family, size, weight and line height
//! each one is set in. Sizes are theme tokens; families come from the theme's `font` section.

use super::tokens::ThemeTokens;

/// One of the theme's three families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSlot {
    /// Sansation: the wordmark and headings, at 21 px and up only.
    Display,
    /// Source Sans 3: all interface text.
    Body,
    /// Ubuntu Mono: every number.
    Mono,
}

/// A tier of the type scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeRole {
    Display,
    Headline,
    Body,
    Label,
    Numeral,
    NumeralLarge,
    /// The fader dB scale ticks, the one text below the label size.
    Scale,
}

impl TypeRole {
    pub const ALL: [TypeRole; 7] = [
        TypeRole::Display,
        TypeRole::Headline,
        TypeRole::Body,
        TypeRole::Label,
        TypeRole::Numeral,
        TypeRole::NumeralLarge,
        TypeRole::Scale,
    ];

    pub fn slot(self) -> FontSlot {
        match self {
            TypeRole::Display | TypeRole::Headline => FontSlot::Display,
            TypeRole::Body | TypeRole::Label => FontSlot::Body,
            TypeRole::Numeral | TypeRole::NumeralLarge | TypeRole::Scale => FontSlot::Mono,
        }
    }

    /// CSS-style weight (DESIGN.md): 700 display, 400 body, 500 labels and numerals.
    pub fn weight(self) -> u16 {
        match self {
            TypeRole::Display | TypeRole::Headline => 700,
            TypeRole::Body => 400,
            TypeRole::Label | TypeRole::Numeral | TypeRole::NumeralLarge | TypeRole::Scale => 500,
        }
    }

    /// Line height as a multiple of the size.
    pub fn line_height(self) -> f32 {
        match self {
            TypeRole::Display => 1.1,
            TypeRole::Headline => 1.236,
            TypeRole::Body => 1.5,
            TypeRole::Label => 1.2,
            TypeRole::Numeral | TypeRole::NumeralLarge | TypeRole::Scale => 1.0,
        }
    }
}

/// Everything a front end needs to set one tier.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeStyle {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
}

/// Smallest size (px) for the display family, for words, and for running prose (DESIGN.md).
pub const MIN_DISPLAY_SIZE: f32 = 21.0;
pub const MIN_LABEL_SIZE: f32 = 11.0;
pub const MIN_PROSE_SIZE: f32 = 12.0;

impl ThemeTokens {
    /// The family name a slot is set in.
    pub fn family(&self, slot: FontSlot) -> &str {
        match slot {
            FontSlot::Display => &self.font_display,
            FontSlot::Body => &self.font_body,
            FontSlot::Mono => &self.font_mono,
        }
    }

    /// The size token of a tier (px).
    pub fn type_size(&self, role: TypeRole) -> f32 {
        match role {
            TypeRole::Display => self.type_display,
            TypeRole::Headline => self.type_headline,
            TypeRole::Body => self.type_body,
            TypeRole::Label => self.type_label,
            TypeRole::Numeral => self.type_numeral,
            TypeRole::NumeralLarge => self.type_numeral_large,
            TypeRole::Scale => self.type_scale,
        }
    }

    pub fn type_style(&self, role: TypeRole) -> TypeStyle {
        TypeStyle {
            family: self.family(role.slot()).to_owned(),
            size: self.type_size(role),
            weight: role.weight(),
            line_height: role.line_height(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{parse_theme, BUILTIN_THEMES};

    fn builtins() -> Vec<(&'static str, ThemeTokens)> {
        BUILTIN_THEMES
            .iter()
            .map(|(name, json)| (*name, parse_theme(json).expect("parses").tokens))
            .collect()
    }

    #[test]
    fn the_built_in_themes_use_the_design_scale_and_families() {
        for (name, t) in builtins() {
            let sizes = TypeRole::ALL.map(|r| t.type_size(r));
            assert_eq!(sizes, [34.0, 21.0, 13.0, 11.0, 13.0, 21.0, 10.0], "{name}");
            assert_eq!(t.family(FontSlot::Display), "Sansation", "{name}");
            assert_eq!(t.family(FontSlot::Body), "Source Sans 3", "{name}");
            assert_eq!(t.family(FontSlot::Mono), "Ubuntu Mono", "{name}");
        }
    }

    #[test]
    fn sansation_never_goes_below_21_px() {
        for (name, t) in builtins() {
            for role in TypeRole::ALL
                .into_iter()
                .filter(|r| r.slot() == FontSlot::Display)
            {
                assert!(t.type_size(role) >= MIN_DISPLAY_SIZE, "{name}: {role:?}");
            }
        }
    }

    #[test]
    fn no_label_below_11_no_prose_below_12_and_only_the_scale_at_10() {
        for (name, t) in builtins() {
            assert!(t.type_body >= MIN_PROSE_SIZE, "{name}: body");
            for role in TypeRole::ALL.into_iter().filter(|r| *r != TypeRole::Scale) {
                assert!(t.type_size(role) >= MIN_LABEL_SIZE, "{name}: {role:?}");
            }
            assert_eq!(t.type_scale, 10.0, "{name}: the fader dB scale");
        }
    }

    #[test]
    fn each_tier_has_the_design_weight_and_family_slot() {
        let t = ThemeTokens::builtin_dark();
        let display = t.type_style(TypeRole::Display);
        assert_eq!(
            (display.family.as_str(), display.weight),
            ("Sansation", 700)
        );
        assert_eq!(t.type_style(TypeRole::Body).weight, 400);
        assert_eq!(t.type_style(TypeRole::Label).weight, 500);
        let numeral = t.type_style(TypeRole::NumeralLarge);
        assert_eq!(
            (numeral.family.as_str(), numeral.weight),
            ("Ubuntu Mono", 500)
        );
        assert_eq!(TypeRole::Scale.slot(), FontSlot::Mono);
    }
}
