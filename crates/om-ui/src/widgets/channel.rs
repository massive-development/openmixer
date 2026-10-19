//! One mixer channel: trim, filter, a three-band EQ with kill switches, PFL, the FX shortcut,
//! crossfader assignment, the channel fader and a stereo meter. Routes `mixer.ch<N>.<id>`. State, routing and layout.

use super::{px, CAPTION_HEIGHT};
use crate::binding::{assign_from_index, assign_index, LevelSource, MixerBinding};
use crate::control::{Knob, Ranged, Readout};
use crate::geometry::Rect;
use crate::meter::{MeterParams, StereoMeter};
use crate::theme::ThemeTokens;
use om_core::param_id::{ChannelParam, EqBand};
use om_dsp::CrossfaderAssign;

/// EQ bands in engine order: low, mid, high.
pub const BANDS: usize = 3;
pub const EQ_BANDS: [EqBand; BANDS] = [EqBand::Low, EqBand::Mid, EqBand::High];
pub const EQ_LABELS: [&str; BANDS] = ["Lo", "Mid", "Hi"];
/// Assignment buttons in display order: Thru ("Mid"), Left, Right.
pub const ASSIGN_LABELS: [&str; 3] = ["Mid", "L", "R"];
/// Kill drops a band to this floor; a band at or below it reads as killed.
pub const KILL_TOLERANCE_DB: f64 = 0.01;

/// Rectangles of one channel strip.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelLayout {
    pub title: Rect,
    pub trim: Rect,
    pub filter: Rect,
    /// Indexed by band (engine order), though displayed Hi | Mid | Lo.
    pub eq: [Rect; BANDS],
    pub kill: [Rect; BANDS],
    pub pfl: Rect,
    pub fx: Rect,
    /// Indexed by [`CrossfaderAssign`] index: Thru, Left, Right.
    pub assign: [Rect; 3],
    pub fader_caption: Rect,
    pub fader: Rect,
    pub meter: Rect,
}

pub struct ChannelStrip {
    binding: MixerBinding,
    channel: usize,
    pub trim: Knob,
    pub filter: Knob,
    pub eq: [Knob; BANDS],
    pub fader: Ranged,
    pub pfl: bool,
    /// Crossfader assignment index (see [`assign_index`]).
    pub assign: usize,
    pub meter: StereoMeter,
    /// EQ value to return to when a kill is released.
    restore_db: [f64; BANDS],
}

impl ChannelStrip {
    /// Builds the controls and reads the engine once. Nothing is sent.
    pub fn new(binding: MixerBinding, channel: usize, meter: MeterParams) -> Self {
        let min_db = f64::from(om_dsp::ThreeBandEq::MIN_DB);
        let max_db = f64::from(om_dsp::ThreeBandEq::MAX_DB);
        let mut strip = Self {
            binding,
            channel,
            trim: Knob::new("Trim", Readout::Gain, 0.0, 4.0, 1.0),
            filter: Knob::new("Filter", Readout::Filter, -1.0, 1.0, 0.0),
            eq: EQ_LABELS.map(|label| Knob::new(label, Readout::Decibels, min_db, max_db, 0.0)),
            fader: Ranged::new(0.0, 1.0, 0.001, 1.0),
            pfl: false,
            assign: 0,
            meter: StereoMeter::new(meter),
            restore_db: [0.0; BANDS],
        };
        strip.pull(false);
        strip
    }

    pub fn channel(&self) -> usize {
        self.channel
    }

    /// Reads the engine into every control. The fader is left alone while the user drags it.
    pub fn pull(&mut self, dragging_fader: bool) {
        let m = self.binding.mixer();
        let ch = self.channel;
        self.trim.set_quiet(f64::from(m.trim(ch)));
        self.filter.set_quiet(f64::from(m.filter(ch)));
        for (band, knob) in self.eq.iter_mut().enumerate() {
            knob.set_quiet(f64::from(m.eq(ch, band)));
        }
        if !dragging_fader {
            self.fader.set(f64::from(m.fader(ch)));
        }
        self.pfl = m.pfl(ch);
        self.assign = assign_index(m.crossfader_assign(ch));
    }

    /// One UI frame: pulls the engine state and advances the meter by `dt` seconds.
    pub fn refresh(&mut self, dt: f32, levels: &dyn LevelSource, dragging_fader: bool) {
        self.pull(dragging_fader);
        let (left, right) = levels.channel_peaks(self.channel);
        self.meter.update(left, right, dt);
    }

    fn send(&self, p: ChannelParam, value: f64) {
        self.binding.send_channel(self.channel, p, value);
    }

    pub fn trim_changed(&mut self, value: f64) -> f64 {
        let v = self.trim.range.set(value);
        self.send(ChannelParam::Trim, v);
        v
    }

    pub fn filter_changed(&mut self, value: f64) -> f64 {
        let v = self.filter.range.set(value);
        self.send(ChannelParam::Filter, v);
        v
    }

    pub fn eq_changed(&mut self, band: usize, value: f64) -> f64 {
        let v = self.eq[band].range.set(value);
        self.send(ChannelParam::Eq(EQ_BANDS[band]), v);
        v
    }

    /// The fader moved: stores the snapped value and sends it.
    pub fn fader_changed(&mut self, value: f64) -> f64 {
        let v = self.fader.set(value);
        self.send(ChannelParam::Fader, v);
        v
    }

    /// Whether a band is killed (at the engine floor).
    pub fn is_killed(&self, band: usize) -> bool {
        f64::from(self.binding.mixer().eq(self.channel, band))
            <= f64::from(om_dsp::ThreeBandEq::MIN_DB) + KILL_TOLERANCE_DB
    }

    /// Kill drops the band to the floor and remembers where it was; releasing goes back there.
    pub fn kill_pressed(&mut self, band: usize) {
        if self.is_killed(band) {
            self.send(ChannelParam::Eq(EQ_BANDS[band]), self.restore_db[band]);
        } else {
            self.restore_db[band] = f64::from(self.binding.mixer().eq(self.channel, band));
            self.send(
                ChannelParam::Eq(EQ_BANDS[band]),
                f64::from(om_dsp::ThreeBandEq::MIN_DB),
            );
        }
        self.pull(false);
    }

    /// PFL toggles the channel into the headphone cue.
    pub fn pfl_pressed(&mut self) -> bool {
        let on = !self.binding.mixer().pfl(self.channel);
        self.send(ChannelParam::Pfl, if on { 1.0 } else { 0.0 });
        self.pull(false);
        on
    }

    /// Crossfader assignment buttons (index as [`assign_index`]). Out-of-range indices do nothing.
    pub fn assign_pressed(&mut self, index: usize) -> bool {
        let Some(assign) = assign_from_index(index) else {
            return false;
        };
        self.send(ChannelParam::Xfader, assign_index(assign) as f64);
        self.pull(false);
        true
    }

    /// The FX button: the caller shows this channel's rack.
    pub fn fx_pressed(&self) -> usize {
        self.channel
    }

    /// The crossfader assignment the engine reports, for display.
    pub fn assignment(&self) -> CrossfaderAssign {
        self.binding.mixer().crossfader_assign(self.channel)
    }

    /// Rectangles of the strip for a `bounds` card. Mirrors `ChannelStripWidget::resized`.
    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> ChannelLayout {
        let gap = theme.inner_gap / 2;
        let mut area = bounds.reduced(px(theme.mixer_pad_x), px(theme.mixer_pad_y));
        let title = area
            .remove_top(px(theme.mixer_title_height))
            .with_trimmed_left(14);
        area.remove_top(gap);

        // Knobs: trim | filter, then three EQ bands, each with a kill button under it.
        let knob_h = px(theme.mixer_knob_height);
        let mut top = area.remove_top(knob_h);
        let trim = top.remove_left(top.w / 2);
        let filter = top;
        let mut row = area.remove_top(knob_h);
        let cell = row.w / BANDS as i32;
        let mut kill_row = area.remove_top(px(theme.mixer_button_height) - 4);
        let mut eq = [Rect::default(); BANDS];
        let mut kill = [Rect::default(); BANDS];
        for i in 0..BANDS {
            // Displayed Hi | Mid | Lo, as on a hardware mixer.
            let band = BANDS - 1 - i;
            eq[band] = row.remove_left(cell);
            kill[band] = kill_row.remove_left(cell).reduced(1, 0);
        }
        area.remove_top(gap);

        // Buttons: PFL | FX, then the L | Thru | R assignment strip.
        let button_h = px(theme.mixer_button_height);
        let mut row = area.remove_top(button_h);
        let pfl = row.remove_left(row.w / 2).reduced(1, 0);
        let fx = row.reduced(1, 0);
        area.remove_top(gap);
        let mut segments = area.remove_top(button_h);
        let third = segments.w / 3;
        let mut assign = [Rect::default(); 3];
        assign[1] = segments.remove_left(third);
        assign[0] = segments.remove_left(third);
        assign[2] = segments;
        area.remove_top(gap);

        // Fader and meter, centred under the buttons.
        let fader_caption = area.remove_top(CAPTION_HEIGHT);
        let meter_w = px(theme.mixer_meter_width) * 2 + 2;
        let mut group = area.with_size_keeping_centre(
            area.w.min(px(theme.mixer_fader_width) + gap + meter_w),
            area.h,
        );
        let meter = group.remove_right(meter_w);
        group.remove_right(gap);

        ChannelLayout {
            title,
            trim,
            filter,
            eq,
            kill,
            pfl,
            fx,
            assign,
            fader_caption,
            fader: group,
            meter,
        }
    }
}
