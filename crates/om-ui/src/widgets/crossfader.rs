//! Crossfader slider with its curve switch (Linear / Power). Routes `mixer.crossfader` and
//! `mixer.crossfader.curve`; the selector mirrors the engine.

use crate::binding::{curve_from_index, curve_index, MixerBinding};
use crate::control::Ranged;
use crate::geometry::Rect;
use crate::theme::ThemeTokens;
use om_core::param_id::MixerParam;
use om_dsp::CrossfaderCurve;

/// The curve switch's labels, indexed like [`curve_index`]: Linear, Power.
pub const CURVE_LABELS: [&str; 2] = ["LIN", "PWR"];
pub const CURVE_TIPS: [&str; 2] = [
    "Straight blend between the two sides",
    "Constant-power blend: no volume dip in the middle",
];
/// Height of the crossfader row (a primary control), in px.
pub const CROSSFADER_HEIGHT: i32 = 44;
/// Width of the curve switch beside the slider, in px.
pub const CURVE_SWITCH_WIDTH: i32 = 48;

/// Rectangles of the crossfader section: the slider, and one switch that cycles the curve. The
/// section carries no caption; the A and B ends are printed on the slider itself.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfaderLayout {
    pub slider: Rect,
    pub curve: Rect,
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

    /// The curve switch: moves to the other curve and mirrors the engine's answer.
    pub fn curve_cycled(&mut self) -> Option<CrossfaderCurve> {
        self.curve_pressed((self.curve + 1) % CURVE_LABELS.len())
    }

    /// Height the section needs: one row.
    pub fn height_for(_width: i32, _theme: &ThemeTokens) -> i32 {
        CROSSFADER_HEIGHT
    }

    /// Rectangles of the section for a `bounds` card.
    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> CrossfaderLayout {
        let mut area = bounds.reduced(theme.pad, 0);
        let curve = area.remove_right(CURVE_SWITCH_WIDTH.min(area.w / 3));
        area.remove_right(theme.inner_gap.min(area.w));
        CrossfaderLayout {
            slider: area,
            curve,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_has_a_slider_and_a_curve_switch_side_by_side() {
        let theme = ThemeTokens::builtin_dark();
        for w in [200, 213, 320] {
            let l = CrossfaderSection::layout(Rect::sized(w, CROSSFADER_HEIGHT), &theme);
            assert!(
                l.slider.w > l.curve.w,
                "{w}: the slider is the main control"
            );
            assert!(!l.slider.intersects(&l.curve));
            assert!(l.curve.h >= 32 && l.slider.h >= 32);
        }
    }

    #[test]
    fn curve_labels_are_short_codes() {
        assert_eq!(CURVE_LABELS.len(), CURVE_TIPS.len());
        assert!(CURVE_LABELS.iter().all(|l| l.len() == 3));
    }
}
