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

/// Width of the master fader, in px: its 26 px cap, a 2 px gap, a 10 px major tick and the 22 px
/// label gutter, so the +6 / 0 / -6 ... marks print (`faders-meters.md` section 3 master anatomy).
pub const MASTER_FADER_WIDTH: i32 = 26 + 2 + 10 + 22;
/// Gap between the master fader and its meter, in px.
pub const FADER_METER_GAP: i32 = 8;
/// Height of the numeric peak readout under the meter, in px (12 px text, spec).
pub const READOUT_HEIGHT: i32 = 16;
/// Width the section needs: the fader, its gap and the labelled meter, plus the side padding.
pub fn width_for(theme: &ThemeTokens) -> i32 {
    MASTER_FADER_WIDTH
        + FADER_METER_GAP
        + column_width(theme.mixer_meter_width, true)
        + 2 * px(theme.mixer_pad_x)
}

/// Rectangles of the master section: the MASTER caption, the fader and the meter column side by
/// side, and the numeric peak readout under them.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterLayout {
    pub title: Rect,
    pub fader: Rect,
    pub meter: Rect,
    /// The numeric peak readout, under the meter column.
    pub readout: Rect,
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
        let mut area = bounds.reduced(px(theme.mixer_pad_x), 0);
        let title = area.remove_top(CAPTION_HEIGHT);
        area.remove_top(theme.inner_gap / 2);
        let readout = area.remove_bottom(READOUT_HEIGHT);
        area.remove_bottom(theme.inner_gap / 2);
        let with_labels = area.w
            >= MASTER_FADER_WIDTH + FADER_METER_GAP + column_width(theme.mixer_meter_width, true);
        let meter_w = column_width(theme.mixer_meter_width, with_labels);
        let fader_w = MASTER_FADER_WIDTH.min(area.w);
        let mut group =
            area.with_size_keeping_centre(area.w.min(fader_w + FADER_METER_GAP + meter_w), area.h);
        let meter = group.remove_right(meter_w);
        group.remove_right(FADER_METER_GAP);
        MasterLayout {
            title,
            fader: group,
            meter,
            readout,
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

    #[test]
    fn the_master_fader_is_wide_enough_for_labels_and_tall_enough_for_its_ticks() {
        let theme = ThemeTokens::builtin_dark();
        let w = width_for(&theme);
        // The module's upper area at the smallest window: 262 px tall minus the crossfader block.
        let l = MasterSection::layout(Rect::sized(w, 190), &theme);
        assert!(l.meter_labels);
        assert!(l.fader.w >= MASTER_FADER_WIDTH);
        assert!(l.fader.h >= 120, "fader {} px", l.fader.h);
        assert!(l.meter.h == l.fader.h, "meter and fader share their height");
        assert!(l.readout.y >= l.fader.bottom() && l.readout.h >= 16);
        assert!(l.title.bottom() <= l.fader.y);
    }
}
