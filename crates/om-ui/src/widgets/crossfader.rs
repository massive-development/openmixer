//! Crossfader: a wide horizontal fader with A and B printed outside its ends, and a two-state curve
//! switch (Linear / Power) under it. Routes `mixer.crossfader` and `mixer.crossfader.curve`; the
//! selector mirrors the engine. Spec: `docs/design/research/faders-meters.md` section 3.

use crate::binding::{curve_from_index, curve_index, MixerBinding};
use crate::control::Ranged;
use crate::geometry::Rect;
use crate::theme::ThemeTokens;
use om_core::param_id::MixerParam;
use om_dsp::CrossfaderCurve;

/// The curve switch's labels, indexed like [`curve_index`]: Linear, Power. Spelled in full
/// (`labels-layout.md` section 1: "if kept, `Linear` / `Power` in full").
pub const CURVE_LABELS: [&str; 2] = ["Linear", "Power"];
pub const CURVE_TIPS: [&str; 2] = [
    "Straight blend between the two sides",
    "Constant-power blend: no volume dip in the middle",
];
/// Height of the fader row: the cap is 20 px tall (spec) inside a 28 px hit area, in px.
pub const FADER_ROW_HEIGHT: i32 = 28;
/// Width reserved at each end for the A and B letters, which sit outside the fader so they never
/// meet its end ticks, in px.
pub const END_MARK_WIDTH: i32 = 16;
/// Height of the curve switch row (the minimum hit size of `labels-layout.md` section 5), in px.
pub const CURVE_ROW_HEIGHT: i32 = 28;
/// Gap between the fader row and the curve switch, in px.
pub const ROW_GAP: i32 = 6;
/// Height of the crossfader section: the fader row, a gap and the curve switch, in px.
pub const CROSSFADER_HEIGHT: i32 = FADER_ROW_HEIGHT + ROW_GAP + CURVE_ROW_HEIGHT;

/// Rectangles of the crossfader section: the slider with A and B beside its ends, and under it one
/// button per curve (both always visible, the engine's curve lit). No caption.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfaderLayout {
    pub a_mark: Rect,
    pub slider: Rect,
    pub b_mark: Rect,
    /// Indexed like [`curve_index`].
    pub curve: [Rect; 2],
}

pub struct CrossfaderSection {
    binding: MixerBinding,
    pub slider: Ranged,
    curve: usize,
}

impl CrossfaderSection {
    /// Reads the engine once; nothing is sent.
    pub fn new(binding: MixerBinding) -> Self {
        let mut section = Self {
            binding,
            slider: Ranged::new(0.0, 1.0, 0.001, 0.5),
            curve: 0,
        };
        section.pull(false);
        section
    }

    /// Reads the position and curve from the engine. The slider is left alone while dragged.
    pub fn pull(&mut self, dragging: bool) {
        let m = self.binding.mixer();
        if !dragging {
            self.slider.set(f64::from(m.crossfader()));
        }
        self.curve = curve_index(m.crossfader_curve());
    }

    /// The user moved the slider: stores and sends the position.
    pub fn slider_changed(&mut self, value: f64) -> f64 {
        let v = self.slider.set(value);
        self.binding.send(MixerParam::Crossfader, v);
        v
    }

    /// A curve button: sends its index and mirrors the engine's answer.
    pub fn curve_pressed(&mut self, index: usize) -> Option<CrossfaderCurve> {
        let curve = curve_from_index(index)?;
        self.binding
            .send(MixerParam::CrossfaderCurve, curve_index(curve) as f64);
        self.pull(false);
        Some(curve)
    }

    /// Index of the curve the engine uses (see [`curve_index`]).
    pub fn selected_curve(&self) -> usize {
        self.curve
    }

    /// Moves to the other curve and mirrors the engine's answer.
    pub fn curve_cycled(&mut self) -> Option<CrossfaderCurve> {
        self.curve_pressed((self.curve + 1) % CURVE_LABELS.len())
    }

    /// Height the section needs: the fader row, a gap and the curve switch.
    pub fn height_for(_width: i32, _theme: &ThemeTokens) -> i32 {
        CROSSFADER_HEIGHT
    }

    /// Rectangles of the section for a `bounds` card.
    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> CrossfaderLayout {
        let mut area = bounds.reduced(theme.mixer_pad_x as i32, 0);
        let mut row = area.remove_top(FADER_ROW_HEIGHT);
        area.remove_top(ROW_GAP);
        let a_mark = row.remove_left(END_MARK_WIDTH);
        let b_mark = row.remove_right(END_MARK_WIDTH);
        let mut switch = area.remove_top(CURVE_ROW_HEIGHT);
        let half = switch.w / 2;
        let first = switch.remove_left(half).reduced(1, 0);
        CrossfaderLayout {
            a_mark,
            slider: row,
            b_mark,
            curve: [first, switch.reduced(1, 0)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fader_is_the_wide_control_with_a_and_b_outside_its_ends() {
        let theme = ThemeTokens::builtin_dark();
        let pad = theme.mixer_pad_x as i32;
        for w in [200, 233, 320] {
            let l = CrossfaderSection::layout(Rect::sized(w, CROSSFADER_HEIGHT), &theme);
            assert!(l.slider.w >= w - 2 * END_MARK_WIDTH - 2 * pad, "{w}");
            assert!(l.a_mark.right() <= l.slider.x, "{w}: A meets the fader");
            assert!(l.slider.right() <= l.b_mark.x, "{w}: B meets the fader");
            assert!(l.slider.h >= 28, "{w}: the cap's hit area");
        }
    }

    #[test]
    fn both_curves_are_visible_buttons_under_the_fader() {
        let theme = ThemeTokens::builtin_dark();
        let l = CrossfaderSection::layout(Rect::sized(233, CROSSFADER_HEIGHT), &theme);
        for r in l.curve {
            assert!(r.h >= 28 && r.w >= 48, "a curve button is {}x{}", r.w, r.h);
            assert!(r.y >= l.slider.bottom(), "the switch sits under the fader");
        }
        assert!(!l.curve[0].intersects(&l.curve[1]));
        assert!(l.curve[0].x < l.curve[1].x, "Linear left of Power");
    }

    #[test]
    fn curve_labels_are_whole_words() {
        assert_eq!(CURVE_LABELS.len(), CURVE_TIPS.len());
        assert_eq!(CURVE_LABELS, ["Linear", "Power"]);
    }
}
