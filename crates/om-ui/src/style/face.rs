//! Control faces: the state set of a pressable control (button, pad, toggle, segment) from the
//! Design System canvas, section 5 "States" (`openmixer-workbench/docs/design-system`). Depth is a 1px etched edge and
//! a 1px top highlight; an active control is told apart by fill, edge and a word, never by light.
//!
//! The deck tint mixes in OkLab, which keeps the deck's hue at any share. The canvas mixes with CSS
//! `color-mix(in oklch, ..)`; on these near-neutral faces the two differ by a small hue shift only.

use crate::style::{ControlStyle, ShadowStyle};
use crate::theme::{Colour, ThemeTokens};

/// One state of a pressable control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// Control surface, secondary text, etched edge and top highlight.
    Rest,
    /// One tonal step up; text to ivory.
    Hover,
    /// Sinks to the well tone with a small inset shadow.
    Pressed,
    /// Deck-hue tint over the face with a deck-hue edge; loop active and sync locked too.
    On,
    /// The "on" tint and edge raised (32% and 85% in the built-in themes).
    OnHover,
    /// Transport only: a solid deck fill with ground-coloured text.
    Playing,
    /// Panel tone, dashed graphic edge, tertiary text, no hover.
    Disabled,
}

impl Face {
    /// Every state, in the canvas order.
    pub const ALL: [Face; 7] = [
        Face::Rest,
        Face::Hover,
        Face::Pressed,
        Face::On,
        Face::OnHover,
        Face::Playing,
        Face::Disabled,
    ];

    /// The state a pointer over the control shows; disabled and playing do not change.
    pub fn hovered(self) -> Face {
        match self {
            Face::Rest => Face::Hover,
            Face::On => Face::OnHover,
            other => other,
        }
    }

    /// Short name, for the gallery and test messages.
    pub fn name(self) -> &'static str {
        match self {
            Face::Rest => "Rest",
            Face::Hover => "Hover",
            Face::Pressed => "Pressed",
            Face::On => "On",
            Face::OnHover => "On + hover",
            Face::Playing => "Playing",
            Face::Disabled => "Disabled",
        }
    }
}

/// How one state of a control face is drawn. A transparent `highlight` or `sink.colour` means the
/// state has none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceStyle {
    pub fill: Colour,
    pub text: Colour,
    /// The 1px edge: the etched line, a deck-hue edge, or the dashed graphic edge.
    pub edge: Colour,
    pub dashed: bool,
    /// The 1px top highlight inside the edge.
    pub highlight: Colour,
    /// The inset shadow of a press.
    pub sink: ShadowStyle,
    pub edge_width: f32,
    pub radius: f32,
    /// The label is set heavier (playing).
    pub strong: bool,
}

impl FaceStyle {
    /// The face as a plain control style at `font_size`, for contrast checks and simple boxes.
    pub fn control(&self, font_size: f32) -> ControlStyle {
        ControlStyle {
            fill: self.fill,
            text: self.text,
            border: self.edge,
            border_width: self.edge_width,
            radius: self.radius,
            font_size,
        }
    }
}

/// The 2px ring a keyboard-focused control draws, 2px outside its edge. Never removed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingStyle {
    pub colour: Colour,
    pub width: f32,
    pub offset: f32,
}

/// Share of the deck hue a set pad is tinted with, and its edge (canvas 4a, "Pad": 12% and 40%).
pub const PAD_TINT: f32 = 0.12;
pub const PAD_EDGE: f32 = 0.40;
/// Share of the deck hue in a deck tag's edge where it sits on the face (canvas 5, "Deck identity").
pub const IDENTITY_EDGE: f32 = 0.55;

/// The tokens a face is built from, copied out of the theme once per [`super::LookAndFeel`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceTokens {
    ground: Colour,
    panel: Colour,
    face: Colour,
    face_hover: Colour,
    well: Colour,
    line: Colour,
    graphic_edge: Colour,
    highlight: Colour,
    shadow: Colour,
    text: Colour,
    text_secondary: Colour,
    text_muted: Colour,
    tint: (f32, f32),
    tint_hover: (f32, f32),
    edge_width: f32,
    radius: f32,
}

impl FaceTokens {
    pub fn from_tokens(t: &ThemeTokens) -> Self {
        Self {
            ground: t.ground,
            panel: t.surface,
            face: t.surface_raised,
            face_hover: t.surface_hover,
            well: t.well,
            line: t.border.with_multiplied_alpha(t.edge_line_alpha),
            graphic_edge: t.border_strong,
            highlight: t.highlight.with_alpha(t.edge_highlight_alpha),
            shadow: t.shadow,
            text: t.text_primary,
            text_secondary: t.text_secondary,
            text_muted: t.text_muted,
            tint: (t.deck_tint_alpha, t.deck_border_alpha),
            tint_hover: (t.deck_tint_hover_alpha, t.deck_border_hover_alpha),
            edge_width: t.border_width,
            radius: t.control_radius,
        }
    }

    /// The face of a control in state `face`; `hue` is the deck colour an "on" or playing control
    /// takes (the text colour for a control that belongs to no deck).
    pub fn face(&self, face: Face, hue: Colour) -> FaceStyle {
        let base = FaceStyle {
            fill: self.face,
            text: self.text_secondary,
            edge: self.line,
            dashed: false,
            highlight: self.highlight,
            sink: no_shadow(self.shadow),
            edge_width: self.edge_width,
            radius: self.radius,
            strong: false,
        };
        match face {
            Face::Rest => base,
            Face::Hover => FaceStyle {
                fill: self.face_hover,
                text: self.text,
                ..base
            },
            Face::Pressed => FaceStyle {
                fill: self.well,
                text: self.text,
                highlight: self.highlight.with_alpha(0.0),
                sink: self.sink(),
                ..base
            },
            Face::On => self.lit(hue, self.tint, base),
            Face::OnHover => self.lit(hue, self.tint_hover, base),
            Face::Playing => FaceStyle {
                fill: hue,
                text: self.ground,
                edge: hue,
                highlight: self.highlight.with_alpha(0.0),
                strong: true,
                ..base
            },
            Face::Disabled => FaceStyle {
                fill: self.panel,
                text: self.text_muted,
                edge: self.graphic_edge,
                dashed: true,
                highlight: self.highlight.with_alpha(0.0),
                ..base
            },
        }
    }

    /// A set pad: the face with a light deck tint and edge.
    pub fn pad(&self, hue: Colour) -> FaceStyle {
        FaceStyle {
            fill: tint(self.face, hue, PAD_TINT),
            text: self.text,
            edge: tint(self.line, hue, PAD_EDGE),
            highlight: self.highlight.with_alpha(0.0),
            ..self.face(Face::Rest, hue)
        }
    }

    /// An empty pad: no fill of its own over the panel, the etched edge, muted text.
    pub fn empty_pad(&self) -> FaceStyle {
        FaceStyle {
            fill: Colour::mix(&[(self.face, 0.4), (self.panel, 0.6)]),
            text: self.text_muted,
            highlight: self.highlight.with_alpha(0.0),
            ..self.face(Face::Rest, self.text)
        }
    }

    /// The deck letter chip: the solid deck hue with the letter in the ground colour.
    pub fn deck_tag(&self, hue: Colour) -> FaceStyle {
        self.face(Face::Playing, hue)
    }

    /// An unselected segment of a segmented control: no fill (the group shows through), muted text.
    pub fn segment(&self, selected: bool, hue: Colour) -> FaceStyle {
        if selected {
            return FaceStyle {
                edge: self.line,
                ..self.face(Face::On, hue)
            };
        }
        FaceStyle {
            fill: self.face.with_alpha(0.0),
            text: self.text_muted,
            highlight: self.highlight.with_alpha(0.0),
            ..self.face(Face::Rest, hue)
        }
    }

    fn lit(&self, hue: Colour, (fill, edge): (f32, f32), base: FaceStyle) -> FaceStyle {
        FaceStyle {
            fill: tint(self.face, hue, fill),
            text: self.text,
            edge: tint(self.line, hue, edge),
            highlight: self.highlight.with_alpha(0.0),
            ..base
        }
    }

    /// The press: `inset 0 1px 2px` in the shadow colour.
    fn sink(&self) -> ShadowStyle {
        ShadowStyle {
            colour: self.shadow,
            blur: 2.0,
            offset_y: 1.0,
        }
    }
}

fn no_shadow(colour: Colour) -> ShadowStyle {
    ShadowStyle {
        colour: colour.with_alpha(0.0),
        blur: 0.0,
        offset_y: 0.0,
    }
}

/// `hue` mixed into `base` at `share` (0..1) in OkLab; alpha mixes linearly.
pub fn tint(base: Colour, hue: Colour, share: f32) -> Colour {
    let s = share.clamp(0.0, 1.0);
    let (p, q) = (oklab(base), oklab(hue));
    let mixed = [0, 1, 2].map(|i| p[i] * (1.0 - s) + q[i] * s);
    let alpha = base.alpha() * (1.0 - s) + hue.alpha() * s;
    from_oklab(mixed).with_alpha(alpha)
}

fn linear(channel: u32) -> f32 {
    let v = channel as f32 / 255.0;
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn encoded(v: f32) -> u32 {
    let v = v.clamp(0.0, 1.0);
    let e = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (e * 255.0).round() as u32
}

/// OkLab `[L, a, b]` of an sRGB colour (alpha ignored).
pub fn oklab(c: Colour) -> [f32; 3] {
    let argb = c.argb();
    let (r, g, b) = (
        linear((argb >> 16) & 0xFF),
        linear((argb >> 8) & 0xFF),
        linear(argb & 0xFF),
    );
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn from_oklab([l, a, b]: [f32; 3]) -> Colour {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let r = 4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_;
    let g = -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_;
    let bl = -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_;
    Colour::from_argb(0xFF00_0000 | (encoded(r) << 16) | (encoded(g) << 8) | encoded(bl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{contrast_ratio, over};

    fn builtin(id: &str) -> ThemeTokens {
        let (_, json) = crate::theme::BUILTIN_THEMES
            .iter()
            .find(|(file, _)| file.contains(id))
            .expect("built-in theme present");
        crate::theme::parse_theme(json)
            .expect("built-in theme parses")
            .tokens
    }

    const THEMES: [&str; 3] = ["dark", "daylight", "harbour"];

    #[test]
    fn oklab_round_trips_and_tint_ends_are_the_inputs() {
        for argb in [
            0xFF12_1211,
            0xFFE3_A24F,
            0xFF86_A9CB,
            0xFFFF_FFFF,
            0xFF00_0000,
        ] {
            let c = Colour::from_argb(argb);
            assert_eq!(from_oklab(oklab(c)), c, "{argb:08X}");
        }
        let (face, amber) = (
            Colour::from_argb(0xFF22_211E),
            Colour::from_argb(0xFFE3_A24F),
        );
        assert_eq!(tint(face, amber, 0.0), face);
        assert_eq!(tint(face, amber, 1.0), amber);
    }

    #[test]
    fn a_deck_tint_keeps_the_hue_and_rises_with_the_share() {
        let t = ThemeTokens::builtin_dark();
        let f = FaceTokens::from_tokens(&t);
        let slate = t.accent_for(1);
        let on = f.face(Face::On, slate);
        // Slate is blue: the tinted face leans to blue (b < 0 in OkLab), unlike the bare face.
        assert!(oklab(on.fill)[2] < oklab(t.surface_raised)[2]);
        let hover = f.face(Face::OnHover, slate);
        let lum = |c: Colour| oklab(c)[0];
        assert!(lum(hover.fill) > lum(on.fill) && lum(on.fill) > lum(t.surface_raised));
        assert!(lum(hover.edge) > lum(on.edge));
    }

    #[test]
    fn the_states_follow_the_canvas() {
        let t = ThemeTokens::builtin_dark();
        let f = FaceTokens::from_tokens(&t);
        let amber = t.accent_for(0);
        let rest = f.face(Face::Rest, amber);
        assert_eq!((rest.fill, rest.text), (t.surface_raised, t.text_secondary));
        assert!(!rest.highlight.is_transparent() && rest.sink.colour.is_transparent());
        let hover = f.face(Face::Hover, amber);
        assert_eq!((hover.fill, hover.text), (t.surface_hover, t.text_primary));
        let pressed = f.face(Face::Pressed, amber);
        assert_eq!(pressed.fill, t.well);
        assert!(!pressed.sink.colour.is_transparent() && pressed.highlight.is_transparent());
        let playing = f.face(Face::Playing, amber);
        assert_eq!(
            (playing.fill, playing.text, playing.edge),
            (amber, t.ground, amber)
        );
        assert!(playing.strong);
        let off = f.face(Face::Disabled, amber);
        assert_eq!(
            (off.fill, off.text, off.edge),
            (t.surface, t.text_muted, t.border_strong)
        );
        assert!(off.dashed);
        assert_eq!(Face::Rest.hovered(), Face::Hover);
        assert_eq!(Face::On.hovered(), Face::OnHover);
        assert_eq!(Face::Disabled.hovered(), Face::Disabled);
    }

    #[test]
    fn every_enabled_state_keeps_its_label_readable_in_every_theme() {
        for id in THEMES {
            let t = builtin(id);
            let f = FaceTokens::from_tokens(&t);
            let ground = over(t.surface, t.ground);
            for hue in t.accents.clone() {
                for face in Face::ALL {
                    let s = f.face(face, hue);
                    let fill = over(s.fill, ground);
                    let r = contrast_ratio(over(s.text, fill), fill);
                    let floor = if face == Face::Disabled { 3.0 } else { 4.5 };
                    assert!(r >= floor, "{id}: {} label {r:.2} < {floor}", face.name());
                }
            }
        }
    }

    #[test]
    fn the_disabled_edge_and_the_focus_ring_hold_three_to_one() {
        for id in THEMES {
            let t = builtin(id);
            let f = FaceTokens::from_tokens(&t);
            let panel = over(t.surface, t.ground);
            let off = f.face(Face::Disabled, t.accent_for(0));
            assert!(contrast_ratio(off.edge, panel) >= 3.0, "{id}: dashed edge");
            for surface in [t.ground, panel, over(t.surface_raised, panel)] {
                assert!(contrast_ratio(t.focus_ring, surface) >= 3.0, "{id}: ring");
            }
        }
    }

    #[test]
    fn a_deck_tag_reads_on_its_hue_in_every_theme() {
        for id in THEMES {
            let t = builtin(id);
            let f = FaceTokens::from_tokens(&t);
            for hue in t.accents.clone() {
                let tag = f.deck_tag(hue);
                assert!(
                    contrast_ratio(tag.text, tag.fill) >= 4.5,
                    "{id}: tag letter"
                );
            }
        }
    }

    #[test]
    fn pads_and_segments_differ_by_state() {
        let t = ThemeTokens::builtin_dark();
        let f = FaceTokens::from_tokens(&t);
        let hue = t.accent_for(0);
        assert_ne!(f.pad(hue).fill, f.empty_pad().fill);
        assert_ne!(f.pad(hue).edge, f.empty_pad().edge);
        assert_ne!(f.segment(true, hue).fill, f.segment(false, hue).fill);
        assert!(f.segment(false, hue).fill.is_transparent());
    }
}
