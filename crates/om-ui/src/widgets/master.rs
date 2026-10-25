//! Master section: the master fader (up to the engine's gain ceiling) and its stereo meter. Routes
//! `mixer.master`.

use super::{px, CAPTION_HEIGHT};
use crate::binding::{LevelSource, MixerBinding};
use crate::control::{Knob, Readout};
use crate::geometry::Rect;
use crate::meter::{column_width, MeterParams, StereoMeter};
use crate::theme::ThemeTokens;
use om_core::param_id::MixerParam;
use om_engine::mixer::MAX_GAIN;

/// Rectangles of the master section: the MASTER caption, the fader and the meter column.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterLayout {
    pub title: Rect,
    pub fader: Rect,
    pub meter: Rect,
    /// Whether the meter column has room for its dB labels.
    pub meter_labels: bool,
}

pub struct MasterSection {
    binding: MixerBinding,
    /// Master gain as a knob; unity is its detent.
    pub fader: Knob,
    pub meter: StereoMeter,
}

impl MasterSection {
    pub fn new(binding: MixerBinding, meter: MeterParams) -> Self {
        let mut section = Self {
            binding,
            fader: Knob::new("MASTER", Readout::Gain, 0.0, f64::from(MAX_GAIN), 1.0).with_detent(),
            meter: StereoMeter::new(meter),
        };
        section.pull(false);
        section
    }

    pub fn pull(&mut self, dragging: bool) {
        if !dragging {
            self.fader
                .set_quiet(f64::from(self.binding.mixer().master_gain()));
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
        let v = self.fader.range.set(value);
        self.binding.send(MixerParam::Master, v);
        v
    }

    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> MasterLayout {
        let mut area = bounds.reduced(px(theme.mixer_pad_x), px(theme.mixer_pad_y));
        let title = area.remove_top(CAPTION_HEIGHT);
        area.remove_top(theme.inner_gap / 2);
        let fader_w = px(theme.mixer_fader_width);
        let gap = theme.inner_gap;
        let with_labels = area.w >= fader_w + gap + column_width(theme.mixer_meter_width, true);
        let meter_w = column_width(theme.mixer_meter_width, with_labels);
        let mut group = area.with_size_keeping_centre(area.w.min(fader_w + gap + meter_w), area.h);
        let meter = group.remove_right(meter_w);
        group.remove_right(gap);
        MasterLayout {
            title,
            fader: group,
            meter,
            meter_labels: with_labels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fader_and_meter_sit_side_by_side_under_the_title() {
        let theme = ThemeTokens::builtin_dark();
        let l = MasterSection::layout(Rect::sized(213, 150), &theme);
        assert!(l.title.bottom() <= l.fader.y && l.title.bottom() <= l.meter.y);
        assert!(!l.fader.intersects(&l.meter));
        assert!(l.meter_labels, "213 px is room for the scale");
        assert!(l.fader.h >= 80);
    }
}
