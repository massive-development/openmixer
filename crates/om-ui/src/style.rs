//! Look and feel: framework-free control styles derived from [`ThemeTokens`].
//!
//! Every colour, radius, border width, font size and inset a control draws comes from a token, so
//! switching the theme restyles every control. Nothing here names a literal colour or size. The
//! gpui front end converts a [`ControlStyle`] into paint calls; the contrast helpers let tests check
//! that a theme leaves its control text readable.
//!
//! How to style a control: add a field to [`LookAndFeel`] (or a sub-struct) built in
//! [`LookAndFeel::from_tokens`] from existing tokens, add a row to [`LookAndFeel::entries`] so the
//! live-switch test covers it, and draw with the style in the view. If a value is missing, add a
//! token (see `theme/tokens.rs`) with its default in both built-in theme files.

use crate::theme::{Colour, ThemeTokens};

/// The drawing recipe of one control state: fill, text, border and shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlStyle {
    pub fill: Colour,
    pub text: Colour,
    pub border: Colour,
    pub border_width: f32,
    pub radius: f32,
    pub font_size: f32,
}

impl ControlStyle {
    /// WCAG contrast ratio of this style's text over its fill, with the fill and text composited
    /// over `backdrop` first. Use [`contrast_ratio`] for the maths.
    pub fn contrast_on(&self, backdrop: Colour) -> f32 {
        let ground = over(self.fill, backdrop);
        contrast_ratio(over(self.text, ground), ground)
    }
}

/// Button states. `lit` is built per call with [`LookAndFeel::lit_button`] because its fill is the
/// caller's accent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonStyles {
    pub idle: ControlStyle,
    pub hover: ControlStyle,
    pub active: ControlStyle,
    pub disabled: ControlStyle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnobStyle {
    /// Knob body; its label and readout sit on top of it.
    pub body: ControlStyle,
    /// The value fill rising from the bottom of the body.
    pub fill: Colour,
    pub label: ControlStyle,
    pub readout: ControlStyle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaderStyle {
    pub track: ControlStyle,
    pub thumb: ControlStyle,
    pub thumb_size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabStyles {
    pub selected: ControlStyle,
    pub unselected: ControlStyle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextFieldStyles {
    pub idle: ControlStyle,
    pub focus: ControlStyle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeterStyle {
    pub well: Colour,
    pub low: Colour,
    pub mid: Colour,
    pub high: Colour,
    pub lamp_on: Colour,
    pub lamp_off: Colour,
    pub radius: f32,
    pub headroom: f32,
    pub lamp_height: f32,
    pub inset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeparatorStyle {
    pub colour: Colour,
    pub thickness: f32,
}

/// Every control style for one theme. Built once per theme change; cheap to rebuild.
#[derive(Debug, Clone, PartialEq)]
pub struct LookAndFeel {
    pub button: ButtonStyles,
    /// Lit buttons take the accent as their fill; see [`LookAndFeel::lit_button`].
    pub lit_text: Colour,
    pub knob: KnobStyle,
    pub fader: FaderStyle,
    pub tabs: TabStyles,
    pub tooltip: ControlStyle,
    pub text_field: TextFieldStyles,
    pub meter: MeterStyle,
    pub panel: ControlStyle,
    pub panel_caption: ControlStyle,
    pub separator: SeparatorStyle,
    /// Tint for a drop target under a dragged track.
    pub drop_target: Colour,
    /// Font size of the body text used in panels and tooltips.
    pub body_size: f32,
    /// Insets (px) inside a control's chrome.
    pub inset_small: f32,
    pub inset_large: f32,
    pub bar_radius: f32,
    /// Disabled text colour is the muted text colour at this alpha.
    pub disabled_alpha: f32,
    pub accent: Colour,
}

impl LookAndFeel {
    /// Derives every control style from `t`.
    pub fn from_tokens(t: &ThemeTokens) -> Self {
        let accent = t.accent_for(0);
        let raised = t.surface_raised_scaled();
        let shape =
            |fill: Colour, text: Colour, border: Colour, radius: f32, size: f32| ControlStyle {
                fill,
                text,
                border,
                border_width: t.border_width,
                radius,
                font_size: size,
            };
        let button = |fill: Colour, text: Colour| {
            shape(fill, text, t.border, t.control_radius, t.type_small)
        };
        let disabled_text = t.text_muted.with_multiplied_alpha(t.disabled_alpha);
        Self {
            button: ButtonStyles {
                idle: button(raised, t.text_primary),
                hover: button(t.surface_hover_scaled(), t.text_primary),
                active: button(t.well, t.text_primary),
                disabled: button(t.surface_scaled(), disabled_text),
            },
            lit_text: t.on_accent,
            knob: KnobStyle {
                body: shape(
                    raised,
                    t.text_primary,
                    t.border,
                    t.control_radius,
                    t.type_caption,
                ),
                fill: accent,
                label: shape(
                    raised,
                    t.text_secondary,
                    t.border,
                    t.control_radius,
                    t.type_caption,
                ),
                readout: shape(
                    raised,
                    t.text_primary,
                    t.border,
                    t.control_radius,
                    t.type_caption,
                ),
            },
            fader: FaderStyle {
                track: shape(
                    t.well,
                    t.text_primary,
                    t.border,
                    t.control_radius,
                    t.type_caption,
                ),
                thumb: shape(
                    t.cap_top,
                    t.text_primary,
                    t.highlight,
                    t.control_radius,
                    t.type_caption,
                ),
                thumb_size: t.thumb_size,
            },
            tabs: TabStyles {
                selected: shape(
                    t.surface_hover_scaled(),
                    t.text_primary,
                    accent,
                    t.control_radius,
                    t.type_small,
                ),
                unselected: shape(
                    t.surface_scaled(),
                    t.text_secondary,
                    t.border,
                    t.control_radius,
                    t.type_small,
                ),
            },
            tooltip: shape(
                t.popup,
                t.text_primary,
                t.border,
                t.control_radius,
                t.type_caption,
            ),
            text_field: TextFieldStyles {
                idle: shape(
                    t.well,
                    t.text_primary,
                    t.border,
                    t.control_radius,
                    t.type_body,
                ),
                focus: ControlStyle {
                    border: accent,
                    border_width: t.focus_border_width,
                    ..shape(
                        t.well,
                        t.text_primary,
                        accent,
                        t.control_radius,
                        t.type_body,
                    )
                },
            },
            meter: MeterStyle {
                well: t.well,
                low: t.meter_low,
                mid: t.meter_mid,
                high: t.meter_high,
                lamp_on: t.meter_high,
                lamp_off: raised,
                radius: t.bar_radius,
                headroom: t.meter_headroom,
                lamp_height: t.lamp_height,
                inset: t.inset_small,
            },
            panel: shape(
                t.surface_scaled(),
                t.text_primary,
                t.border,
                t.card_radius,
                t.type_body,
            ),
            panel_caption: shape(
                t.surface_scaled(),
                t.text_secondary,
                t.border,
                t.card_radius,
                t.type_caption,
            ),
            separator: SeparatorStyle {
                colour: t.border,
                thickness: t.border_width,
            },
            drop_target: t.drop_target,
            body_size: t.type_body,
            inset_small: t.inset_small,
            inset_large: t.inset_large,
            bar_radius: t.bar_radius,
            disabled_alpha: t.disabled_alpha,
            accent,
        }
    }

    /// A lit button: the caller's accent as fill, with the theme's on-accent text.
    pub fn lit_button(&self, accent: Colour) -> ControlStyle {
        ControlStyle {
            fill: accent,
            text: self.lit_text,
            ..self.button.idle
        }
    }

    /// Every text-bearing control style, named, for contrast and live-switch checks.
    pub fn entries(&self) -> Vec<(&'static str, ControlStyle)> {
        vec![
            ("button.idle", self.button.idle),
            ("button.hover", self.button.hover),
            ("button.active", self.button.active),
            ("button.disabled", self.button.disabled),
            ("button.lit", self.lit_button(self.accent)),
            ("knob.body", self.knob.body),
            ("knob.label", self.knob.label),
            ("knob.readout", self.knob.readout),
            ("fader.thumb", self.fader.thumb),
            ("tabs.selected", self.tabs.selected),
            ("tabs.unselected", self.tabs.unselected),
            ("tooltip", self.tooltip),
            ("text_field.idle", self.text_field.idle),
            ("text_field.focus", self.text_field.focus),
            ("panel", self.panel),
            ("panel.caption", self.panel_caption),
        ]
    }
}

/// `top` composited over `below` (source-over). The result is opaque when `below` is.
pub fn over(top: Colour, below: Colour) -> Colour {
    let a = top.alpha();
    let mix = |t: u32, b: u32| ((t as f32 * a) + (b as f32 * (1.0 - a))).round() as u32;
    let chan = |shift: u32| mix((top.argb() >> shift) & 0xFF, (below.argb() >> shift) & 0xFF);
    let alpha = below.alpha() + top.alpha() * (1.0 - below.alpha());
    let a8 = (alpha * 255.0).round() as u32;
    Colour::from_argb((a8 << 24) | (chan(16) << 16) | (chan(8) << 8) | chan(0))
}

/// WCAG relative luminance of an opaque colour.
pub fn relative_luminance(c: Colour) -> f32 {
    let lin = |v: u32| {
        let s = v as f32 / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let argb = c.argb();
    0.2126 * lin((argb >> 16) & 0xFF) + 0.7152 * lin((argb >> 8) & 0xFF) + 0.0722 * lin(argb & 0xFF)
}

/// WCAG contrast ratio (1..=21) between two opaque colours. Order does not matter.
pub fn contrast_ratio(a: Colour, b: Colour) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_of_black_and_white_is_twenty_one() {
        let r = contrast_ratio(
            Colour::from_argb(0xFFFF_FFFF),
            Colour::from_argb(0xFF00_0000),
        );
        assert!((r - 21.0).abs() < 0.01, "{r}");
    }

    #[test]
    fn compositing_respects_alpha() {
        let half_white = over(
            Colour::from_argb(0x80FF_FFFF),
            Colour::from_argb(0xFF00_0000),
        );
        let v = (half_white.argb() & 0xFF) as i32;
        assert!((v - 128).abs() <= 1, "{v}");
        assert_eq!(
            over(
                Colour::from_argb(0x00FF_FFFF),
                Colour::from_argb(0xFF00_0000)
            )
            .argb(),
            0xFF00_0000
        );
    }

    fn builtin(id: &str) -> ThemeTokens {
        let (_, json) = crate::theme::BUILTIN_THEMES
            .iter()
            .find(|(file, _)| file.contains(id))
            .expect("built-in theme present");
        crate::theme::parse_theme(json)
            .expect("built-in theme parses")
            .tokens
    }

    /// Lowest contrast of `style`'s text over its fill, checked on both ends of the backdrop gradient.
    fn worst_contrast(style: &ControlStyle, t: &ThemeTokens) -> f32 {
        [t.backdrop_top, t.backdrop_bottom]
            .into_iter()
            .map(|b| style.contrast_on(b))
            .fold(f32::MAX, f32::min)
    }

    #[test]
    fn both_builtin_themes_keep_control_text_readable() {
        for id in ["dark", "daylight"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            for (name, style) in look.entries() {
                if name.starts_with("button.disabled") {
                    // Inactive controls are exempt from WCAG text contrast.
                    continue;
                }
                let ratio = worst_contrast(&style, &t);
                assert!(ratio >= 3.0, "{id}: {name} contrast {ratio:.2} < 3.0");
            }
            for accent in t.accents.clone() {
                let ratio = worst_contrast(&look.lit_button(accent), &t);
                assert!(
                    ratio >= 3.0,
                    "{id}: lit button on {:08X} contrast {ratio:.2}",
                    accent.argb()
                );
            }
        }
    }

    #[test]
    fn both_builtin_themes_keep_body_text_at_aa() {
        for id in ["dark", "daylight"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            for (name, style) in [
                ("panel", look.panel),
                ("panel.caption", look.panel_caption),
                ("tooltip", look.tooltip),
            ] {
                let ratio = worst_contrast(&style, &t);
                assert!(ratio >= 4.5, "{id}: {name} body contrast {ratio:.2} < 4.5");
            }
        }
    }

    #[test]
    fn switching_theme_changes_every_control_style() {
        let dark = LookAndFeel::from_tokens(&builtin("dark"));
        let day = LookAndFeel::from_tokens(&builtin("daylight"));
        for ((name, a), (_, b)) in dark.entries().into_iter().zip(day.entries()) {
            assert_ne!(a, b, "{name} did not change between themes");
        }
        assert_ne!(dark.knob.fill, day.knob.fill);
        assert_ne!(dark.meter.well, day.meter.well);
        assert_ne!(dark.drop_target, day.drop_target);
        assert_ne!(dark.separator.colour, day.separator.colour);
    }

    #[test]
    fn every_style_is_derived_from_tokens_with_no_empty_text() {
        let t = ThemeTokens::builtin_dark();
        let look = LookAndFeel::from_tokens(&t);
        for (name, style) in look.entries() {
            assert!(style.font_size > 0.0, "{name} has no font size");
            assert!(style.radius >= 0.0, "{name} has a negative radius");
        }
        assert_eq!(look.lit_button(look.accent).fill, look.accent);
    }
}
