//! Headphone cue section: the cue/master blend and the phones level. Routes `mixer.cue.mix` and
//! `mixer.cue.level`.

use super::CAPTION_HEIGHT;
use crate::binding::MixerBinding;
use crate::control::{Knob, Readout};
use crate::geometry::Rect;
use crate::theme::ThemeTokens;
use om_core::param_id::MixerParam;
use om_engine::mixer::MAX_GAIN;

/// Height of the phones row: the PHONES caption and a 32 px slider row, in px.
pub const CUE_HEIGHT: i32 = CAPTION_HEIGHT + 32;
/// Short names printed inside the two sliders.
pub const MIX_LABEL: &str = "MIX";
pub const LEVEL_LABEL: &str = "VOL";

/// Rectangles of the cue section: one PHONES caption over the blend and level sliders.
#[derive(Debug, Clone, PartialEq)]
pub struct CueLayout {
    pub caption: Rect,
    pub mix: Rect,
    pub level: Rect,
}

pub struct CueSection {
    binding: MixerBinding,
    /// Headphone blend: cue channels only (0) to master only (1). Double-click returns to 0.
    pub mix: Knob,
    /// Phones volume; unity is its detent.
    pub level: Knob,
}

impl CueSection {
    pub fn new(binding: MixerBinding) -> Self {
        let mut section = Self {
            binding,
            mix: Knob::new(MIX_LABEL, Readout::Percent, 0.0, 1.0, 0.0),
            level: Knob::new(LEVEL_LABEL, Readout::Gain, 0.0, f64::from(MAX_GAIN), 1.0)
                .with_detent(),
        };
        section.pull(false, false);
        section
    }

    pub fn pull(&mut self, mix_dragging: bool, level_dragging: bool) {
        let m = self.binding.mixer();
        if !mix_dragging {
            self.mix.set_quiet(f64::from(m.cue_mix()));
        }
        if !level_dragging {
            self.level.set_quiet(f64::from(m.cue_level()));
        }
    }

    pub fn mix_changed(&mut self, value: f64) -> f64 {
        let v = self.mix.range.set(value);
        self.binding.send(MixerParam::CueMix, v);
        v
    }

    pub fn level_changed(&mut self, value: f64) -> f64 {
        let v = self.level.range.set(value);
        self.binding.send(MixerParam::CueLevel, v);
        v
    }

    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> CueLayout {
        let mut area = bounds.reduced(theme.pad, 0);
        let caption = area.remove_top(CAPTION_HEIGHT);
        let half = (area.w - theme.inner_gap) / 2;
        let mix = area.remove_left(half);
        area.remove_left(theme.inner_gap);
        CueLayout {
            caption,
            mix,
            level: area,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sliders_share_the_row_under_one_caption() {
        let theme = ThemeTokens::builtin_dark();
        let l = CueSection::layout(Rect::sized(213, CUE_HEIGHT), &theme);
        assert!(l.caption.bottom() <= l.mix.y);
        assert!(!l.mix.intersects(&l.level));
        assert!(l.mix.h >= 32 && l.level.h >= 32);
    }
}
