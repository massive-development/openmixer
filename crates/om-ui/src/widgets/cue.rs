//! Headphone cue section: the cue/master blend and the phones level. Routes `mixer.cue.mix` and
//! `mixer.cue.level`.

use super::CAPTION_HEIGHT;
use crate::binding::MixerBinding;
use crate::control::Ranged;
use crate::geometry::{place_captioned, Rect};
use crate::theme::ThemeTokens;
use om_engine::mixer::MAX_GAIN;

/// Rectangles of the cue section.
#[derive(Debug, Clone, PartialEq)]
pub struct CueLayout {
    pub mix_caption: Rect,
    pub mix: Rect,
    pub level_caption: Rect,
    pub level: Rect,
}

pub struct CueSection {
    binding: MixerBinding,
    /// Headphone blend: cue channels only (0) to master only (1). Double-click returns to 0.
    pub mix: Ranged,
    pub level: Ranged,
}

impl CueSection {
    pub fn new(binding: MixerBinding) -> Self {
        let mut section = Self {
            binding,
            mix: Ranged::new(0.0, 1.0, 0.001, 0.0),
            level: Ranged::new(0.0, f64::from(MAX_GAIN), 0.001, 1.0),
        };
        section.pull(false, false);
        section
    }

    pub fn pull(&mut self, mix_dragging: bool, level_dragging: bool) {
        let m = self.binding.mixer();
        if !mix_dragging {
            self.mix.set(f64::from(m.cue_mix()));
        }
        if !level_dragging {
            self.level.set(f64::from(m.cue_level()));
        }
    }

    pub fn mix_changed(&mut self, value: f64) -> f64 {
        let v = self.mix.set(value);
        self.binding.send("cue.mix", v);
        v
    }

    pub fn level_changed(&mut self, value: f64) -> f64 {
        let v = self.level.set(value);
        self.binding.send("cue.level", v);
        v
    }

    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> CueLayout {
        let mut area = bounds.reduced(theme.pad, theme.pad - 4);
        let half = (area.w - theme.gap) / 2;
        let (mix_caption, mix) = place_captioned(area.remove_left(half), CAPTION_HEIGHT);
        area.remove_left(theme.gap);
        let (level_caption, level) = place_captioned(area, CAPTION_HEIGHT);
        CueLayout {
            mix_caption,
            mix,
            level_caption,
            level,
        }
    }
}
