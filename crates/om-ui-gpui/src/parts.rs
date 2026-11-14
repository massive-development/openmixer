//! The faceplate's control parts in one fixed state each: button, pad, segmented control, deck tag
//! chip, and the state words (LOCK, CLIP). Each takes its state from om-ui's [`Face`] model and its
//! sizes from a [`DensityMetrics`], so the same code draws Roomy and Compact. These builders hold
//! no behaviour: the controls gallery forces each state with them, and `controls::face_button`
//! adds the pointer, keyboard and focus behaviour on top.

use crate::font::{mark, numeral, MarkSize, Numeral, TypeExt as _};
use crate::paint::{face, focus_layer, paint};
use gpui::{div, prelude::*, px, relative, Div, FontWeight, SharedString};
use om_ui::style::{Face, FaceStyle, LookAndFeel};
use om_ui::theme::{Colour, DensityMetrics, ThemeTokens, TypeRole};

/// The golden ratio the canvas sizes the deck tag and a pad's width by.
const GOLDEN: f32 = 1.618;
/// A playing transport button is at least this many control heights wide (canvas "Playing").
const PLAYING_MIN_WIDTHS: f32 = 3.0;
/// Line height of a pad's name (canvas "Pad": 1.1).
const PAD_NAME_LINE: f32 = 1.1;
/// Share of the ground colour's alpha the number on a playing pad keeps (canvas "Pad": 70%).
const PLAYING_NUMBER_ALPHA: f32 = 0.7;

/// What a pad holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadState {
    /// Nothing set: the etched edge over the panel, the number only.
    Empty,
    /// A cue or sample is set: a light deck tint and edge, the name in ivory.
    Set,
    /// Playing from it: the solid deck fill with ground-coloured text.
    Playing,
    /// No track on the deck: the disabled face.
    Disabled,
}

impl PadState {
    pub const ALL: [PadState; 4] = [
        PadState::Empty,
        PadState::Set,
        PadState::Playing,
        PadState::Disabled,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PadState::Empty => "Empty",
            PadState::Set => "Set",
            PadState::Playing => "Playing",
            PadState::Disabled => "Disabled",
        }
    }
}

/// A label set in the control-label tier, heavier when the face asks for it (playing).
pub(crate) fn label(t: &ThemeTokens, style: &FaceStyle, text: impl Into<SharedString>) -> Div {
    let el = div().type_role(t, TypeRole::Label).child(text.into());
    if style.strong {
        el.font_weight(FontWeight::SEMIBOLD)
    } else {
        el
    }
}

/// A face box with `text` centred in it. No size: the caller's rect or the builder sets it.
pub fn face_box(t: &ThemeTokens, style: &FaceStyle, text: impl Into<SharedString>) -> Div {
    face(div(), style)
        .flex()
        .items_center()
        .justify_center()
        .whitespace_nowrap()
        .child(label(t, style, text))
}

/// A push button of the density's control height, in state `state` (`hue` lights "on" and
/// "playing").
pub fn button_face(
    t: &ThemeTokens,
    m: &DensityMetrics,
    text: &str,
    state: Face,
    hue: Colour,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let style = look.face(state, hue);
    let el = face_box(t, &style, text.to_owned())
        .flex_none()
        .h(px(m.control_height))
        .px(px(m.space[3]));
    if state == Face::Playing {
        el.min_w(px(m.control_height * PLAYING_MIN_WIDTHS))
    } else {
        el
    }
}

/// `el` with its keyboard focus ring forced on: the gallery's "focus" state. `radius` is the
/// control's corner.
pub fn focused(t: &ThemeTokens, el: Div, radius: f32) -> Div {
    let ring = LookAndFeel::from_tokens(t).focus;
    el.relative().child(focus_layer(ring, radius, None))
}

/// A hot cue or sampler pad: the number at the top in mono, the name under it. `hue` is the deck
/// colour it is tinted and filled with.
pub fn pad_face(
    t: &ThemeTokens,
    m: &DensityMetrics,
    (number, name): (&str, &str),
    state: PadState,
    hue: Colour,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let style = FaceStyle {
        radius: m.panel_radius,
        ..match state {
            PadState::Empty => look.faces.empty_pad(),
            PadState::Set => look.faces.pad(hue),
            PadState::Playing => look.face(Face::Playing, hue),
            PadState::Disabled => look.face(Face::Disabled, hue),
        }
    };
    let number_colour = match state {
        PadState::Playing => style.text.with_multiplied_alpha(PLAYING_NUMBER_ALPHA),
        PadState::Disabled => style.text,
        _ => t.text_muted,
    };
    let mut el = face(div(), &style)
        .flex()
        .flex_col()
        .justify_between()
        .flex_none()
        .h(px(m.pad_size))
        .w(px((m.pad_size * GOLDEN).round()))
        .px(px(m.space[2]))
        .py(px(m.space[1]))
        .child(numeral(t, Numeral::Value, number.to_owned()).text_color(paint(number_colour)));
    if state != PadState::Empty && !name.is_empty() {
        // The canvas sets a pad's name at line height 1.1, so it fits the 34 px Compact pad.
        let text = div()
            .type_role(t, TypeRole::Body)
            .line_height(relative(PAD_NAME_LINE))
            .overflow_hidden()
            .text_ellipsis()
            .child(name.to_owned());
        el = el.child(if style.strong {
            text.font_weight(FontWeight::SEMIBOLD)
        } else {
            text
        });
    }
    el
}

/// A segmented control: one bordered group of `labels` divided by 1px lines, `selected` lit in
/// `hue` (the text colour for a control that belongs to no deck).
pub fn segmented_face(
    t: &ThemeTokens,
    m: &DensityMetrics,
    labels: &[&str],
    selected: Option<usize>,
    hue: Colour,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let group = look.faces.segment(false, hue);
    let mut el = div()
        .flex()
        .flex_none()
        .h(px(m.control_height))
        .rounded(px(group.radius))
        .border_1()
        .border_color(paint(group.edge))
        .overflow_hidden();
    for (i, text) in labels.iter().enumerate() {
        let style = look.faces.segment(selected == Some(i), hue);
        let mut seg = div()
            .flex()
            .items_center()
            .h_full()
            .px(px(m.space[3]))
            .bg(paint(style.fill))
            .text_color(paint(style.text))
            .whitespace_nowrap()
            .child(label(t, &style, (*text).to_owned()));
        if i > 0 {
            seg = seg.border_l_1().border_color(paint(group.edge));
        }
        el = el.child(seg);
    }
    el
}

/// The deck tag chip: the deck letter in mono bold on the solid deck hue, a square the numeral size
/// times the golden ratio (21 px).
pub fn deck_tag(t: &ThemeTokens, letter: &str, hue: Colour) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let style = look.faces.deck_tag(hue);
    let side = (t.type_numeral * GOLDEN).round();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(side))
        .rounded(px(style.radius))
        .bg(paint(style.fill))
        .text_color(paint(style.text))
        .child(mark(t, MarkSize::Letter, letter.to_owned()))
}

/// A state word in mono bold beside the control it qualifies (LOCK in the deck hue, CLIP in signal
/// red), so the state reads without colour.
pub fn state_word(t: &ThemeTokens, word: &str, colour: Colour) -> Div {
    mark(t, MarkSize::Word, word.to_owned()).text_color(paint(colour))
}

/// A toggle with its state word: the "on" face and, while `word` is set, the word in the hue.
pub fn toggle_with_word(
    t: &ThemeTokens,
    m: &DensityMetrics,
    text: &str,
    (state, word): (Face, Option<&str>),
    hue: Colour,
) -> Div {
    let row = div()
        .flex()
        .items_center()
        .gap(px(m.space[1]))
        .child(button_face(t, m, text, state, hue));
    match word {
        Some(w) => row.child(state_word(t, w, hue)),
        None => row,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::theme::Density;

    #[test]
    fn the_deck_tag_is_twenty_one_px() {
        let t = ThemeTokens::builtin_dark();
        assert_eq!((t.type_numeral * GOLDEN).round(), 21.0);
    }

    #[test]
    fn a_pad_is_the_golden_width_of_its_density_height() {
        let t = ThemeTokens::builtin_dark();
        let roomy = t.metrics(Density::Roomy);
        let compact = t.metrics(Density::Compact);
        assert_eq!((roomy.pad_size * GOLDEN).round(), 89.0);
        assert_eq!((compact.pad_size * GOLDEN).round(), 55.0);
    }
}
