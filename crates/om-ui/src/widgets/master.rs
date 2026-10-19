//! Master section: the master fader (up to the engine's gain ceiling) and its stereo meter. Routes
//! `mixer.master`. Port of `src/ui/MasterSection.cpp`.

use super::{px, CAPTION_HEIGHT};
use crate::binding::{LevelSource, MixerBinding};
use crate::control::Ranged;
use crate::geometry::Rect;
use crate::meter::{MeterParams, StereoMeter};
use crate::theme::ThemeTokens;
use om_engine::mixer::MAX_GAIN;

/// Rectangles of the master section.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterLayout {
    pub title: Rect,
    pub fader_caption: Rect,
    pub meter_caption: Rect,
    pub fader: Rect,
    pub meter: Rect,
}

pub struct MasterSection {
    binding: MixerBinding,
    pub fader: Ranged,
    pub meter: StereoMeter,
}

impl MasterSection {
    pub fn new(binding: MixerBinding, meter: MeterParams) -> Self {
        let mut section = Self {
            binding,
            fader: Ranged::new(0.0, f64::from(MAX_GAIN), 0.001, 1.0),
            meter: StereoMeter::new(meter),
        };
        section.pull(false);
        section
    }

    pub fn pull(&mut self, dragging: bool) {
        if !dragging {
            self.fader
                .set(f64::from(self.binding.mixer().master_gain()));
        }
    }

    /// One UI frame: pulls the fader and advances the master meter.
    pub fn refresh(&mut self, dt: f32, levels: &dyn LevelSource, dragging: bool) {
        self.pull(dragging);
        let (left, right) = levels.master_peaks();
        self.meter.update(left, right, dt);
    }

    /// The user moved the fader: stores and sends the gain.
    pub fn fader_changed(&mut self, value: f64) -> f64 {
        let v = self.fader.set(value);
        self.binding.send("master", v);
        v
    }

    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> MasterLayout {
        let gap = theme.inner_gap / 2;
        let mut area = bounds.reduced(px(theme.mixer_pad_x), px(theme.mixer_pad_y));
        let title = area.remove_top(px(theme.mixer_title_height));
        area.remove_top(gap);
        let mut meter_column = area.remove_right(px(theme.mixer_meter_width) * 2 + 2);
        let meter_caption = meter_column.remove_top(CAPTION_HEIGHT).expanded(6, 0);
        let meter = meter_column;
        area.remove_right(gap);
        let fader_caption = area.remove_top(CAPTION_HEIGHT);
        let fader = area.with_size_keeping_centre(area.w.min(px(theme.mixer_fader_width)), area.h);
        MasterLayout {
            title,
            fader_caption,
            meter_caption,
            fader,
            meter,
        }
    }
}
