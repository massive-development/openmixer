//! Crossfader slider with its curve selector (Linear / Power). Routes `mixer.crossfader` and
//! `mixer.crossfader.curve`; the selector mirrors the engine.

use super::{px, CAPTION_HEIGHT};
use crate::binding::{curve_from_index, curve_index, MixerBinding};
use crate::control::Ranged;
use crate::geometry::{place_captioned, Rect};
use crate::theme::ThemeTokens;
use om_core::param_id::MixerParam;
use om_dsp::CrossfaderCurve;

/// Curve buttons in display order: Linear, Power.
pub const CURVE_LABELS: [&str; 2] = ["Linear", "Power"];
pub const CURVE_TIPS: [&str; 2] = [
    "Straight blend between the two sides",
    "Constant-power blend: no volume dip in the middle",
];

/// Rectangles of the crossfader section.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfaderLayout {
    pub caption: Rect,
    pub curve_caption: Rect,
    pub slider: Rect,
    pub curves: [Rect; 2],
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

    /// Narrower than this the curve buttons go under the slider instead of beside it.
    pub const STACK_BELOW_WIDTH: i32 = 300;

    /// Height the section needs at `width`: the slider row, plus a button row when stacked.
    pub fn height_for(width: i32, theme: &ThemeTokens) -> i32 {
        let base = px(theme.mixer_crossfader_height);
        if width < Self::STACK_BELOW_WIDTH {
            base + px(theme.mixer_button_height) + theme.inner_gap
        } else {
            base
        }
    }

    /// Rectangles of the section for a `bounds` card. Mirrors `CrossfaderSection::resized`.
    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> CrossfaderLayout {
        if bounds.w < Self::STACK_BELOW_WIDTH {
            let mut area = bounds.reduced(theme.pad, theme.pad - 4);
            let mut segments = area.remove_bottom(px(theme.mixer_button_height));
            area.remove_bottom(theme.inner_gap);
            let first = segments.remove_left(segments.w / 2);
            let (caption, slider) = place_captioned(area, CAPTION_HEIGHT);
            return CrossfaderLayout {
                caption,
                curve_caption: Rect::default(),
                slider,
                curves: [first, segments],
            };
        }
        let mut area = bounds.reduced(theme.pad, theme.pad - 4);
        let mut right = area.remove_right(px(theme.mixer_curve_width));
        let curve_caption = right.remove_top(CAPTION_HEIGHT);
        let mut segments = right.remove_top(px(theme.mixer_button_height));
        let first = segments.remove_left(segments.w / 2);
        area.remove_right(theme.gap);
        let (caption, slider) = place_captioned(area, CAPTION_HEIGHT);
        CrossfaderLayout {
            caption,
            curve_caption,
            slider,
            curves: [first, segments],
        }
    }
}
