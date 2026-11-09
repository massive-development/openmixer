//! One mixer channel: gain, a three-band EQ whose labels are the kill switches, filter, PFL, the
//! channel fader and its meter, and the crossfader assignment (Thru, A or B). Routes
//! `mixer.ch<N>.<id>`. State and routing; the mixer column places it (`mixer::column_layout`).

use crate::binding::{assign_from_index, assign_index, LevelSource, MixerBinding};
use crate::control::{Knob, Ranged, Readout};
use crate::labels::channel_labels;
use crate::meter::{MeterParams, StereoMeter};
use om_core::param_id::{ChannelParam, EqBand};
use om_dsp::CrossfaderAssign;

/// EQ bands in engine order: low, mid, high.
pub const BANDS: usize = 3;
pub const EQ_BANDS: [EqBand; BANDS] = [EqBand::Low, EqBand::Mid, EqBand::High];
/// Band names as printed on the strip, in engine order. The label is also the kill switch.
pub const EQ_LABELS: [&str; BANDS] = [
    channel_labels().low,
    channel_labels().mid,
    channel_labels().high,
];
/// Knob labels: printed once, 2 to 6 letters.
pub const TRIM_LABEL: &str = channel_labels().gain;
pub const FILTER_LABEL: &str = channel_labels().filter;
/// Crossfader assignment switch, indexed by [`CrossfaderAssign`] index: Thru, Left, Right. On
/// screen it reads A | THRU | B, the sides the crossfader's ends name.
pub const ASSIGN_LABELS: [&str; 3] = ["THRU", "A", "B"];
/// Kill drops a band to this floor; a band at or below it reads as killed.
pub const KILL_TOLERANCE_DB: f64 = 0.01;

/// The letter a channel carries: its deck, `A`, `B`, `C`...
pub fn channel_letter(channel: usize) -> char {
    char::from(b'A' + (channel % 26) as u8)
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
            trim: Knob::new(TRIM_LABEL, Readout::Gain, 0.0, 4.0, 1.0).with_detent(),
            filter: Knob::new(FILTER_LABEL, Readout::Filter, -1.0, 1.0, 0.0).with_detent(),
            eq: EQ_LABELS.map(|label| {
                Knob::new(label, Readout::Decibels, min_db, max_db, 0.0).with_detent()
            }),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_short_and_read_a_thru_b() {
        assert_eq!(EQ_LABELS, ["LOW", "MID", "HIGH"]);
        assert_eq!(ASSIGN_LABELS, ["THRU", "A", "B"]);
        for label in EQ_LABELS
            .iter()
            .chain([&TRIM_LABEL, &FILTER_LABEL])
            .chain(ASSIGN_LABELS.iter())
        {
            assert!(label.len() <= 6 && label.chars().all(|c| c.is_ascii_uppercase()));
        }
        assert_eq!((channel_letter(0), channel_letter(1)), ('A', 'B'));
    }
}
