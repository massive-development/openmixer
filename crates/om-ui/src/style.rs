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

mod face;
pub use face::{
    oklab, tint, Face, FaceStyle, FaceTokens, RingStyle, IDENTITY_EDGE, PAD_EDGE, PAD_TINT,
};

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
    /// The unlit arc a rotary knob's value arc runs along: the etched line (canvas, "Knob").
    pub track: Colour,
    /// The pointer line on a rotary knob's cap.
    pub pointer: Colour,
    /// The one small drop under the raised cap.
    pub drop: ShadowStyle,
    /// The arc and pointer of a knob in its kill zone.
    pub kill: Colour,
}

/// A hardware fader: a recessed slot and a flat two-tone cap (alternate 2px bands of the face
/// and its hover tone) with an etched edge and the larger small drop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaderStyle {
    pub track: ControlStyle,
    pub thumb: ControlStyle,
    pub thumb_band: Colour,
    pub thumb_drop: ShadowStyle,
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
/// A segmented meter: a grey ramp from `low` through `mid` to `top`, `high` (signal red) above the
/// high threshold, and the held CLIP word in `clip`.
pub struct MeterStyle {
    pub well: Colour,
    pub low: Colour,
    pub mid: Colour,
    pub top: Colour,
    pub high: Colour,
    pub clip: Colour,
    pub lamp_on: Colour,
    pub lamp_off: Colour,
    pub border: Colour,
    pub radius: f32,
    pub headroom: f32,
    pub lamp_height: f32,
    /// Top of the clip lamp below the well's edge.
    pub inset: f32,
    /// Side gap between a bar and its well's edge.
    pub bar_inset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeparatorStyle {
    pub colour: Colour,
    pub thickness: f32,
}

/// Scroll bar: a track with a thumb that brightens under the pointer. `width` is in px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollBarStyle {
    pub track: ControlStyle,
    pub thumb: ControlStyle,
    pub thumb_hover: ControlStyle,
    pub width: f32,
}

/// Table chrome: column header, plain and alternate rows, the selected row and the divider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableStyle {
    pub header: ControlStyle,
    pub row: ControlStyle,
    pub row_alt: ControlStyle,
    pub row_selected: ControlStyle,
    pub divider: SeparatorStyle,
}

/// On/off switch: the two track states and the knob that marks the state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToggleStyle {
    pub off: ControlStyle,
    pub on: ControlStyle,
    pub knob: ControlStyle,
}

/// BPM, key and time numerals: big and bright, with a dimmer unit label.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadoutStyle {
    pub value: Colour,
    pub unit: Colour,
    /// Size of the primary numeral (BPM, elapsed time).
    pub value_size: f32,
    /// Size of the companion numeral (key, remaining time).
    pub small_size: f32,
    /// Size of unit labels ("BPM").
    pub unit_size: f32,
}

/// A soft shadow: `colour` blurred over `blur` px, dropped `offset_y` px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowStyle {
    pub colour: Colour,
    pub blur: f32,
    pub offset_y: f32,
}

/// Transition lengths in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionStyle {
    pub fast_ms: f32,
    pub base_ms: f32,
}

/// Every control style for one theme. Built once per theme change; cheap to rebuild.
#[derive(Debug, Clone, PartialEq)]
pub struct LookAndFeel {
    pub button: ButtonStyles,
    /// Text on a solid deck fill (a playing control): the ground colour.
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
    pub scroll_bar: ScrollBarStyle,
    pub table: TableStyle,
    pub toggle: ToggleStyle,
    /// Floating card: the settings overlay and the debug column.
    pub popup: ControlStyle,
    /// Muted text on a popup: footers and hints.
    pub popup_hint: ControlStyle,
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
    pub readout: ReadoutStyle,
    /// The one small drop under raised controls.
    pub lift: ShadowStyle,
    pub motion: MotionStyle,
    /// Keyboard focus ring colour (>= 3:1 on every surface), and the ring a focused control draws.
    pub focus_ring: Colour,
    pub focus: RingStyle,
    /// The tokens every control state is built from; see [`LookAndFeel::face`].
    pub faces: FaceTokens,
    /// Smallest button/pad, and smallest mouse-only target (px).
    pub min_target: f32,
    pub min_pointer_target: f32,
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
        let faces = FaceTokens::from_tokens(t);
        let state = |face: Face| faces.face(face, accent).control(t.type_label);
        let lift = ShadowStyle {
            colour: t.shadow,
            blur: t.shadow_blur,
            offset_y: t.shadow_offset_y,
        };
        Self {
            button: ButtonStyles {
                idle: state(Face::Rest),
                hover: state(Face::Hover),
                active: state(Face::Pressed),
                disabled: state(Face::Disabled),
            },
            lit_text: t.on_accent,
            // The canvas knob: a cap in the hover tone with an etched edge and the small drop.
            knob: KnobStyle {
                body: shape(
                    t.surface_hover_scaled(),
                    t.text_primary,
                    t.border,
                    t.control_radius,
                    t.type_label,
                ),
                fill: accent,
                label: shape(
                    raised,
                    t.text_secondary,
                    t.border_strong,
                    t.control_radius,
                    t.type_label,
                ),
                readout: shape(
                    raised,
                    t.text_primary,
                    t.border_strong,
                    t.control_radius,
                    t.type_numeral,
                ),
                track: t.border,
                pointer: t.text_primary,
                drop: lift,
                kill: t.danger,
            },
            fader: FaderStyle {
                track: shape(
                    t.well,
                    t.text_primary,
                    t.border,
                    t.control_radius,
                    t.type_label,
                ),
                thumb: shape(
                    raised,
                    t.text_primary,
                    t.border,
                    t.control_radius,
                    t.type_label,
                ),
                thumb_band: t.surface_hover_scaled(),
                // DESIGN.md Elevation's larger drop (`0 3px 5px`), on fader caps in the canvas.
                thumb_drop: ShadowStyle {
                    blur: t.shadow_blur * 5.0 / 3.0,
                    offset_y: t.shadow_offset_y * 1.5,
                    ..lift
                },
                thumb_size: t.thumb_size,
            },
            tabs: TabStyles {
                selected: shape(
                    t.surface_hover_scaled(),
                    t.text_primary,
                    accent,
                    t.control_radius,
                    t.type_label,
                ),
                unselected: shape(
                    t.surface_scaled(),
                    t.text_secondary,
                    t.border_strong,
                    t.control_radius,
                    t.type_label,
                ),
            },
            tooltip: shape(
                t.popup,
                t.text_primary,
                t.border,
                t.control_radius,
                t.type_body,
            ),
            text_field: TextFieldStyles {
                idle: shape(
                    t.well,
                    t.text_primary,
                    t.border_strong,
                    t.control_radius,
                    t.type_body,
                ),
                focus: ControlStyle {
                    border: t.focus_ring,
                    border_width: t.focus_border_width,
                    ..shape(
                        t.well,
                        t.text_primary,
                        t.focus_ring,
                        t.control_radius,
                        t.type_body,
                    )
                },
            },
            meter: MeterStyle {
                well: t.well,
                low: t.meter_low,
                mid: t.meter_mid,
                top: t.text_primary,
                high: t.meter_high,
                clip: t.record_red(),
                lamp_on: t.meter_high,
                lamp_off: raised,
                border: t.border_strong,
                radius: t.bar_radius,
                headroom: t.meter_headroom,
                lamp_height: t.lamp_height,
                inset: t.inset_small,
                bar_inset: t.border_width,
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
                t.type_label,
            ),
            separator: SeparatorStyle {
                colour: t.border,
                thickness: t.border_width,
            },
            scroll_bar: ScrollBarStyle {
                track: shape(
                    t.well,
                    t.text_secondary,
                    t.border,
                    t.bar_radius,
                    t.type_label,
                ),
                thumb: shape(
                    t.text_muted,
                    best_text(t.text_muted, t),
                    t.border_strong,
                    t.bar_radius,
                    t.type_label,
                ),
                thumb_hover: shape(
                    t.text_secondary,
                    best_text(t.text_secondary, t),
                    t.border_strong,
                    t.bar_radius,
                    t.type_label,
                ),
                width: t.min_pointer_target / 3.0,
            },
            table: TableStyle {
                header: shape(raised, t.text_secondary, t.border, 0.0, t.type_label),
                row: shape(
                    t.surface_scaled(),
                    t.text_primary,
                    t.border,
                    0.0,
                    t.type_body,
                ),
                // Between the row and the hover colour, so hovering an alternate row still shows.
                row_alt: shape(
                    Colour::mix(&[(t.surface_scaled(), 0.5), (t.surface_hover_scaled(), 0.5)]),
                    t.text_primary,
                    t.border,
                    0.0,
                    t.type_body,
                ),
                row_selected: shape(
                    Colour::mix(&[(t.surface_raised_scaled(), 0.8), (t.focus_ring, 0.2)]),
                    t.text_primary,
                    t.focus_ring,
                    0.0,
                    t.type_body,
                ),
                divider: SeparatorStyle {
                    colour: t.border,
                    thickness: t.border_width,
                },
            },
            toggle: ToggleStyle {
                off: state(Face::Rest),
                on: state(Face::On),
                knob: shape(
                    t.text_primary,
                    best_text(t.text_primary, t),
                    t.border_strong,
                    t.control_radius,
                    t.type_label,
                ),
            },
            popup: shape(
                t.popup,
                t.text_primary,
                t.border,
                t.control_radius,
                t.type_body,
            ),
            popup_hint: shape(
                t.popup,
                t.text_muted,
                t.border,
                t.control_radius,
                t.type_label,
            ),
            drop_target: t.drop_target,
            body_size: t.type_body,
            inset_small: t.inset_small,
            inset_large: t.inset_large,
            bar_radius: t.bar_radius,
            disabled_alpha: t.disabled_alpha,
            accent,
            readout: ReadoutStyle {
                value: t.readout,
                unit: t.readout_dim,
                value_size: t.type_numeral_large,
                small_size: t.type_numeral,
                unit_size: t.type_label,
            },
            lift,
            // Reduced motion collapses every transition to zero length.
            motion: MotionStyle {
                fast_ms: if t.reduced_motion {
                    0.0
                } else {
                    t.motion_fast_ms
                },
                base_ms: if t.reduced_motion {
                    0.0
                } else {
                    t.motion_base_ms
                },
            },
            focus_ring: t.focus_ring,
            focus: RingStyle {
                colour: t.focus_ring,
                width: t.focus_border_width,
                // DESIGN.md: a 2px ring at a 2px offset; the offset follows the ring width.
                offset: t.focus_border_width,
            },
            faces,
            min_target: t.min_target,
            min_pointer_target: t.min_pointer_target,
        }
    }

    /// The face of a control in state `face`; `hue` is the deck colour of an "on" or playing control.
    pub fn face(&self, face: Face, hue: Colour) -> FaceStyle {
        self.faces.face(face, hue)
    }

    /// A toggle in state `on`: the rest face, or the "on" face in the caller's `accent`.
    pub fn toggle(&self, on: bool, accent: Colour) -> ControlStyle {
        if on {
            self.lit_button(accent)
        } else {
            self.toggle.off
        }
    }

    /// A lit button: the "on" face, the caller's accent tinted over the control with its edge.
    pub fn lit_button(&self, accent: Colour) -> ControlStyle {
        self.face(Face::On, accent)
            .control(self.button.idle.font_size)
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
            ("scroll_bar.track", self.scroll_bar.track),
            ("scroll_bar.thumb", self.scroll_bar.thumb),
            ("scroll_bar.thumb_hover", self.scroll_bar.thumb_hover),
            ("table.header", self.table.header),
            ("table.row", self.table.row),
            ("table.row_alt", self.table.row_alt),
            ("table.row_selected", self.table.row_selected),
            ("toggle.off", self.toggle.off),
            ("toggle.on", self.toggle.on),
            ("toggle.knob", self.toggle.knob),
            ("popup", self.popup),
            ("popup.hint", self.popup_hint),
        ]
    }
}

/// Width to reserve beside a table for its scroll bar: the bar's width while it is shown, else 0.
pub fn gutter_width(shown: bool, bar_width: f32) -> f32 {
    if shown {
        bar_width
    } else {
        0.0
    }
}

/// Of the primary text colour and the on-accent colour, the one that reads better on `fill`
/// (judged over the window ground). For marks with no caption, such as a scroll thumb.
fn best_text(fill: Colour, t: &ThemeTokens) -> Colour {
    let ratio = |text: Colour| {
        let ground = over(fill, t.ground);
        contrast_ratio(over(text, ground), ground)
    };
    if ratio(t.text_primary) >= ratio(t.on_accent) {
        t.text_primary
    } else {
        t.on_accent
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

    /// Lowest contrast of `style`'s text over its fill, over the window ground.
    fn worst_contrast(style: &ControlStyle, t: &ThemeTokens) -> f32 {
        style.contrast_on(t.ground)
    }

    #[test]
    fn both_builtin_themes_keep_control_text_readable() {
        for id in ["dark", "daylight", "harbour"] {
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
        for id in ["dark", "daylight", "harbour"] {
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
    fn every_control_is_distinguishable_from_its_background_in_both_themes() {
        // WCAG 1.4.11's 3:1 holds for what identifies a control: the dashed graphic edge of a
        // disabled one, the knob pointer and the fader cap line on their caps, lit meter segments
        // on the well, and a text field's outline. The etched line is decorative (DESIGN.md).
        for id in ["dark", "daylight", "harbour"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            let panel = over(t.surface_scaled(), t.ground);
            let mut found: Vec<String> = Vec::new();
            let mut check_at = |name: &str, r: f32, floor: f32| {
                if r < floor {
                    found.push(format!("{name} {r:.2} < {floor}"));
                }
            };
            let mut check = |name: &str, r: f32| check_at(name, r, 3.0);
            let b = &look.button;
            check(
                "button.disabled edge",
                contrast_ratio(b.disabled.border, panel),
            );
            let cap = over(look.knob.body.fill, panel);
            check("knob.pointer", contrast_ratio(look.knob.pointer, cap));
            let thumb = over(look.fader.thumb.fill, panel);
            for (i, line) in t.accents.iter().chain([&t.text_primary]).enumerate() {
                check(&format!("fader.cap line {i}"), contrast_ratio(*line, thumb));
            }
            let well = over(look.meter.well, panel);
            for (name, c) in [
                ("low", look.meter.low),
                ("mid", look.meter.mid),
                ("top", look.meter.top),
                ("high", look.meter.high),
            ] {
                check(&format!("meter.{name}"), contrast_ratio(c, well));
            }
            let field = over(look.text_field.idle.fill, panel);
            let outline = over(look.text_field.idle.border, field);
            check(
                "text_field",
                contrast_ratio(outline, panel).max(contrast_ratio(field, panel)),
            );
            // A disabled label is still legible.
            // (WCAG exempts inactive controls; this only keeps them from vanishing.)
            let disabled_text = worst_contrast(&b.disabled, &t);
            check_at("button.disabled text", disabled_text, 1.5);
            assert!(found.is_empty(), "{id}: {found:?}");
        }
    }

    #[test]
    fn readouts_are_the_strongest_text_and_larger_than_body() {
        for id in ["dark", "daylight", "harbour"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            let r = look.readout;
            let ground = over(t.surface_scaled(), t.ground);
            let value = contrast_ratio(over(r.value, ground), ground);
            let unit = contrast_ratio(over(r.unit, ground), ground);
            let body = contrast_ratio(over(t.text_primary, ground), ground);
            assert!(value >= 7.0, "{id}: readout contrast {value:.2} < 7");
            assert!(unit >= 4.5, "{id}: readout unit contrast {unit:.2} < 4.5");
            assert!(value + 0.01 >= body, "{id}: readout weaker than body text");
            assert!(r.value_size >= t.type_headline, "{id}: numerals too small");
            assert!(r.small_size >= t.type_body && r.small_size < r.value_size);
            assert!(r.unit_size < r.small_size);
        }
    }

    #[test]
    fn the_lift_is_one_small_drop() {
        for id in ["dark", "daylight", "harbour"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            // DESIGN.md Elevation: `0 2px 3px` (or `0 3px 5px` under a fader cap).
            let cap = look.fader.thumb_drop;
            assert!(cap.blur <= 5.0 && cap.offset_y <= 3.0, "{id}: cap drop");
            assert!(
                look.lift.blur > 0.0 && look.lift.blur <= 5.0,
                "{id}: drop blur"
            );
            assert!(look.lift.offset_y > 0.0 && look.lift.offset_y <= 3.0);
            assert!(look.motion.fast_ms < look.motion.base_ms);
            assert!(look.motion.base_ms <= 300.0, "{id}: motion should be brief");
        }
    }

    #[test]
    fn the_focus_ring_is_two_px_at_a_two_px_offset_and_not_a_deck_colour() {
        for id in ["dark", "daylight", "harbour"] {
            let t = builtin(id);
            let ring = LookAndFeel::from_tokens(&t).focus;
            assert_eq!((ring.width, ring.offset), (2.0, 2.0), "{id}");
            assert!(
                !t.accents.contains(&ring.colour),
                "{id}: ring is a deck hue"
            );
            assert!(contrast_ratio(ring.colour, t.ground) >= 3.0, "{id}");
        }
        assert_ne!(builtin("dark").accents, builtin("daylight").accents);
    }

    #[test]
    fn deck_accents_are_distinct_in_both_themes() {
        for id in ["dark", "daylight", "harbour"] {
            let t = builtin(id);
            for (i, a) in t.accents.iter().enumerate() {
                for b in &t.accents[i + 1..] {
                    assert_ne!(a, b, "{id}: duplicate accent");
                }
            }
        }
    }

    // ---- Opaque-palette contrast and colour-distinctness checks (dark theme) ----

    /// The surfaces text and markers are drawn on.
    fn dark_surfaces(t: &ThemeTokens) -> [(&'static str, Colour); 4] {
        [
            ("ground", t.ground),
            ("surface", t.surface),
            ("surface_raised", t.surface_raised),
            ("well", t.well),
        ]
    }

    #[test]
    fn dark_surfaces_and_text_are_opaque() {
        let t = ThemeTokens::builtin_dark();
        for (name, col) in [
            ("ground", t.ground),
            ("surface", t.surface),
            ("surface_raised", t.surface_raised),
            ("surface_hover", t.surface_hover),
            ("well", t.well),
            ("border", t.border),
            ("border_strong", t.border_strong),
            ("popup", t.popup),
            ("text_primary", t.text_primary),
            ("text_secondary", t.text_secondary),
            ("text_muted", t.text_muted),
        ] {
            assert_eq!(col.alpha(), 1.0, "{name} must be opaque");
        }
    }

    #[test]
    fn every_text_token_is_aa_on_every_surface_it_sits_on() {
        let t = ThemeTokens::builtin_dark();
        let mut surfaces = dark_surfaces(&t).to_vec();
        surfaces.push(("popup", t.popup));
        surfaces.push(("surface_hover", t.surface_hover));
        for (text_name, text, floor) in [
            ("text_primary", t.text_primary, 4.5),
            ("text_secondary", t.text_secondary, 4.5),
            ("text_muted", t.text_muted, 4.5),
            ("readout", t.readout, 4.5),
            ("readout_dim", t.readout_dim, 4.5),
        ] {
            for (surface_name, surface) in &surfaces {
                let r = contrast_ratio(text, *surface);
                assert!(
                    r >= floor,
                    "{text_name} on {surface_name}: {r:.2} < {floor}"
                );
            }
        }
        // The hierarchy holds on the panel (DESIGN.md's own floors are in tests/theme_json.rs).
        let on_panel = |c: Colour| contrast_ratio(c, t.surface);
        assert!(on_panel(t.text_primary) > on_panel(t.text_secondary));
        assert!(on_panel(t.text_secondary) > on_panel(t.text_muted));
        assert!(on_panel(t.text_muted) >= 4.95);
    }

    #[test]
    fn text_on_accent_fills_is_aa() {
        let t = ThemeTokens::builtin_dark();
        for accent in t.accents.iter().chain([
            &t.playhead,
            &t.cue,
            &t.loop_mark,
            &t.sync_on,
            &t.warning,
            &t.danger,
        ]) {
            let r = contrast_ratio(t.on_accent, *accent);
            assert!(r >= 4.5, "on_accent over {:08X}: {r:.2}", accent.argb());
        }
    }

    #[test]
    fn markers_borders_and_focus_ring_hold_3_to_1_on_every_surface() {
        let t = ThemeTokens::builtin_dark();
        let markers: Vec<(&str, Colour)> = vec![
            ("border_strong", t.border_strong),
            ("focus_ring", t.focus_ring),
            ("playhead", t.playhead),
            ("cue", t.cue),
            ("loop_mark", t.loop_mark),
            ("sync_on", t.sync_on),
            ("warning", t.warning),
            ("danger", t.danger),
            ("deck A", t.accent_for(0)),
            ("deck B", t.accent_for(1)),
            ("deck C", t.accent_for(2)),
            ("deck D", t.accent_for(3)),
        ];
        // The raised surface is the lightest ground a marker sits on.
        for (name, col) in markers {
            for (surface_name, surface) in dark_surfaces(&t) {
                let r = contrast_ratio(col, surface);
                assert!(r >= 3.0, "{name} on {surface_name}: {r:.2} < 3.0");
            }
        }
    }

    /// Euclidean distance in Oklab (perceptual; ~0.02 is a just-noticeable difference).
    fn oklab_distance(a: Colour, b: Colour) -> f32 {
        let lab = |c: Colour| {
            let lin = |shift: u32| {
                let s = ((c.argb() >> shift) & 0xFF) as f32 / 255.0;
                if s <= 0.040_45 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            };
            let (r, g, b) = (lin(16), lin(8), lin(0));
            let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
            let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
            let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
            [
                0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
                1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
                0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
            ]
        };
        let (p, q) = (lab(a), lab(b));
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    }

    #[test]
    fn deck_colours_are_distinct_from_each_other_and_from_semantic_colours() {
        // The Low-Band Rule paints the waveform's low band in the deck colour, so decks are no
        // longer kept apart from waveform bands. Colour-blind separation is in tests/theme_json.rs.
        let t = ThemeTokens::builtin_dark();
        let decks: Vec<Colour> = (0..4).map(|i| t.accent_for(i)).collect();
        for (i, d) in decks.iter().enumerate() {
            // Muted hues: amber A and sage C are the closest pair (about 0.11).
            for other in &decks[i + 1..] {
                let dist = oklab_distance(*d, *other);
                assert!(dist >= 0.10, "deck {i} vs another deck: {dist:.3}");
            }
            // Clay D and signal red are the closest pair (about 0.074), which is why deck
            // identity is always paired with the deck letter, never colour alone.
            for sem in [t.cue, t.loop_mark, t.sync_on, t.warning, t.danger] {
                let dist = oklab_distance(*d, sem);
                assert!(
                    dist >= 0.07,
                    "deck {i} vs semantic {:08X}: {dist:.3}",
                    sem.argb()
                );
            }
        }
    }

    #[test]
    fn control_text_is_never_below_the_label_size_and_targets_meet_minimums() {
        let t = ThemeTokens::builtin_dark();
        for (name, style) in LookAndFeel::from_tokens(&t).entries() {
            assert!(
                style.font_size >= t.type_label,
                "{name}: {}",
                style.font_size
            );
        }
        let look = LookAndFeel::from_tokens(&t);
        for prose in [look.tooltip, look.popup, look.panel] {
            assert!(prose.font_size >= crate::theme::MIN_PROSE_SIZE);
        }
        assert_eq!((t.type_numeral_large, t.type_numeral), (21.0, 13.0));
        assert!(t.deck_row_height >= 40.0);
        assert!(t.min_target >= 44.0 && t.min_pointer_target >= 32.0);
        for gap in [t.gap, t.pad, t.inner_gap] {
            assert!(crate::theme::SPACE_SCALE.contains(&(gap as f32)), "{gap}");
        }
    }

    #[test]
    fn reduced_motion_zeroes_transitions() {
        let mut t = ThemeTokens::builtin_dark();
        assert!(LookAndFeel::from_tokens(&t).motion.base_ms > 0.0);
        t.reduced_motion = true;
        let m = LookAndFeel::from_tokens(&t).motion;
        assert_eq!((m.fast_ms, m.base_ms), (0.0, 0.0));
    }

    #[test]
    fn knob_arc_colours_derive_from_existing_tokens() {
        for id in ["dark", "daylight"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            assert_eq!(look.knob.pointer, t.text_primary);
            assert_eq!(look.knob.kill, t.danger);
            assert_eq!(
                look.knob.track, t.border,
                "the canvas track is the etched line"
            );
            assert_eq!(look.knob.body.fill, t.surface_hover);
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
        assert_ne!(dark.knob.track, day.knob.track);
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
        assert_eq!(
            look.lit_button(look.accent).fill,
            tint(t.surface_raised, look.accent, t.deck_tint_alpha)
        );
    }

    #[test]
    fn every_theme_gives_scroll_table_toggle_readable_contrast() {
        let added = [
            "scroll_bar.track",
            "scroll_bar.thumb",
            "scroll_bar.thumb_hover",
            "table.header",
            "table.row",
            "table.row_alt",
            "table.row_selected",
            "toggle.off",
            "toggle.on",
            "toggle.knob",
        ];
        for id in ["dark", "daylight", "harbour"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            let entries = look.entries();
            for name in added {
                let (_, style) = entries
                    .iter()
                    .find(|(n, _)| *n == name)
                    .unwrap_or_else(|| panic!("{id}: entries() lacks {name}"));
                let ratio = worst_contrast(style, &t);
                assert!(ratio >= 3.0, "{id}: {name} contrast {ratio:.2} < 3.0");
            }
            // Body rows hold the body-text floor.
            for (name, style) in [
                ("table.row", look.table.row),
                ("table.row_alt", look.table.row_alt),
                ("table.row_selected", look.table.row_selected),
            ] {
                let ratio = worst_contrast(&style, &t);
                assert!(ratio >= 4.5, "{id}: {name} body contrast {ratio:.2} < 4.5");
            }
            assert!(look.scroll_bar.width >= 4.0, "{id}: scroll bar too thin");
        }
    }

    #[test]
    fn thumb_is_visible_against_track() {
        for id in ["dark", "daylight", "harbour"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            let ground = over(
                look.scroll_bar.track.fill,
                over(t.surface_scaled(), t.ground),
            );
            for (name, thumb) in [
                ("thumb", look.scroll_bar.thumb),
                ("thumb_hover", look.scroll_bar.thumb_hover),
            ] {
                let r = contrast_ratio(over(thumb.fill, ground), ground);
                assert!(r >= 3.0, "{id}: {name} on track {r:.2} < 3.0");
            }
        }
    }

    #[test]
    fn toggle_states_and_table_rows_differ() {
        for id in ["dark", "daylight", "harbour"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            assert_ne!(look.toggle.on.fill, look.toggle.off.fill, "{id}");
            assert_ne!(look.table.row.fill, look.table.row_alt.fill, "{id}");
            assert_ne!(
                look.table.row_alt.fill, t.surface_hover,
                "{id}: hover hidden"
            );
            assert_ne!(
                look.table.row_alt.fill,
                t.surface_hover_scaled(),
                "{id}: hover hidden"
            );
            assert_ne!(look.table.row.fill, look.table.row_selected.fill, "{id}");
        }
    }

    #[test]
    fn popup_text_and_hint_are_readable_in_both_themes() {
        for id in ["dark", "daylight"] {
            let t = builtin(id);
            let look = LookAndFeel::from_tokens(&t);
            let body = worst_contrast(&look.popup, &t);
            assert!(body >= 4.5, "{id}: popup body contrast {body:.2} < 4.5");
            let hint = worst_contrast(&look.popup_hint, &t);
            assert!(hint >= 3.0, "{id}: popup hint contrast {hint:.2} < 3.0");
        }
    }

    #[test]
    fn a_toggle_takes_the_accent_only_when_on() {
        let t = ThemeTokens::builtin_dark();
        let look = LookAndFeel::from_tokens(&t);
        let accent = t.accent_for(1);
        let on = look.toggle(true, accent);
        assert_eq!(on.fill, tint(t.surface_raised, accent, t.deck_tint_alpha));
        assert_eq!(on.border, tint(t.border, accent, t.deck_border_alpha));
        assert_eq!(on.text, t.text_primary);
        assert_eq!(on.radius, t.control_radius);
        assert_eq!(look.toggle(false, accent), look.toggle.off);
        assert_eq!(look.toggle.off, look.button.idle);
    }

    #[test]
    fn scroll_gutter_is_reserved_only_while_the_bar_is_shown() {
        assert_eq!(gutter_width(true, 10.7), 10.7);
        assert_eq!(gutter_width(false, 10.7), 0.0);
    }
}
