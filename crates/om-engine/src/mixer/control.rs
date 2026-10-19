//! Control-thread surface of the mixer: setters and getters, insert queueing and the string-id
//! parameter router that MIDI, the UI and tests share. Ids match C++ `MixerParameters.cpp`.

use super::shared::{
    assign_from_u8, assign_to_u8, ChannelShared, MixerShared, MAX_EQ_DB, MAX_GAIN, MAX_TRIM,
    MIN_EQ_DB,
};
use crate::stage::StageInsert;
use crate::ProcessStage;
use om_core::ParameterTarget;
use om_dsp::{CrossfaderAssign, CrossfaderCurve};
use std::sync::Arc;

pub struct MixerControl {
    shared: Arc<MixerShared>,
    channel_inserts: Vec<StageInsert>,
    master_insert: StageInsert,
}

impl MixerControl {
    pub(super) fn new(
        shared: Arc<MixerShared>,
        channel_inserts: Vec<StageInsert>,
        master_insert: StageInsert,
    ) -> Self {
        Self {
            shared,
            channel_inserts,
            master_insert,
        }
    }

    pub fn num_channels(&self) -> usize {
        self.shared.channels.len()
    }

    /// Out-of-range indices clamp to the nearest channel, as in the C++ `strip()`.
    fn index(&self, ch: usize) -> usize {
        ch.min(self.num_channels() - 1)
    }

    fn strip(&self, ch: usize) -> &ChannelShared {
        &self.shared.channels[self.index(ch)]
    }

    pub fn set_trim(&self, ch: usize, linear_gain: f32) {
        self.strip(ch).trim.store(linear_gain.clamp(0.0, MAX_TRIM));
    }

    pub fn set_fader(&self, ch: usize, value: f32) {
        self.strip(ch).fader.store(value.clamp(0.0, 1.0));
    }

    pub fn set_eq(&self, ch: usize, band: usize, db: f32) {
        self.strip(ch).eq[band.min(2)].store(db.clamp(MIN_EQ_DB, MAX_EQ_DB));
    }

    pub fn set_filter(&self, ch: usize, position: f32) {
        self.strip(ch).filter.store(position.clamp(-1.0, 1.0));
    }

    pub fn set_pfl(&self, ch: usize, on: bool) {
        self.strip(ch)
            .pfl
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_crossfader_assign(&self, ch: usize, assign: CrossfaderAssign) {
        self.strip(ch)
            .assign
            .store(assign_to_u8(assign), std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_crossfader(&self, position: f32) {
        self.shared.crossfader.store(position.clamp(0.0, 1.0));
    }

    pub fn set_crossfader_curve(&self, curve: CrossfaderCurve) {
        self.shared.constant_power.store(
            curve == CrossfaderCurve::ConstantPower,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    pub fn set_master_gain(&self, gain: f32) {
        self.shared.master_gain.store(gain.clamp(0.0, MAX_GAIN));
    }

    pub fn set_cue_level(&self, gain: f32) {
        self.shared.cue_level.store(gain.clamp(0.0, MAX_GAIN));
    }

    /// 0 = cue only, 1 = master only.
    pub fn set_cue_mix(&self, mix: f32) {
        self.shared.cue_mix.store(mix.clamp(0.0, 1.0));
    }

    pub fn fader(&self, ch: usize) -> f32 {
        self.strip(ch).fader.load()
    }

    pub fn filter(&self, ch: usize) -> f32 {
        self.strip(ch).filter.load()
    }

    pub fn eq(&self, ch: usize, band: usize) -> f32 {
        self.strip(ch).eq[band.min(2)].load()
    }

    pub fn pfl(&self, ch: usize) -> bool {
        self.strip(ch)
            .pfl
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn trim(&self, ch: usize) -> f32 {
        self.strip(ch).trim.load()
    }

    pub fn crossfader_assign(&self, ch: usize) -> CrossfaderAssign {
        assign_from_u8(
            self.strip(ch)
                .assign
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    pub fn crossfader(&self) -> f32 {
        self.shared.crossfader.load()
    }

    pub fn crossfader_curve(&self) -> CrossfaderCurve {
        if self
            .shared
            .constant_power
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            CrossfaderCurve::ConstantPower
        } else {
            CrossfaderCurve::Linear
        }
    }

    pub fn master_gain(&self) -> f32 {
        self.shared.master_gain.load()
    }

    pub fn cue_level(&self) -> f32 {
        self.shared.cue_level.load()
    }

    pub fn cue_mix(&self) -> f32 {
        self.shared.cue_mix.load()
    }

    /// Decaying peaks written by the audio thread; lock-free reads for the meters.
    pub fn channel_peak_left(&self, ch: usize) -> f32 {
        self.strip(ch).meter_left.get()
    }

    pub fn channel_peak_right(&self, ch: usize) -> f32 {
        self.strip(ch).meter_right.get()
    }

    pub fn channel_peak(&self, ch: usize) -> f32 {
        self.channel_peak_left(ch).max(self.channel_peak_right(ch))
    }

    pub fn master_peak_left(&self) -> f32 {
        self.shared.master_peak_left.get()
    }

    pub fn master_peak_right(&self) -> f32 {
        self.shared.master_peak_right.get()
    }

    /// Inserts or clears a stage in a channel chain. Slot 0 belongs to the FX racks.
    pub fn set_channel_insert(
        &self,
        ch: usize,
        slot: usize,
        stage: Option<Box<dyn ProcessStage>>,
    ) -> bool {
        self.channel_inserts[self.index(ch)].set(slot, stage)
    }

    pub fn set_master_insert(&self, slot: usize, stage: Option<Box<dyn ProcessStage>>) -> bool {
        self.master_insert.set(slot, stage)
    }

    fn set_channel_parameter(&self, head: &str, rest: &[&str], value: f64) -> bool {
        let digits = &head[2..];
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let Ok(ch) = digits.parse::<usize>() else {
            return false;
        };
        if ch >= self.num_channels() {
            return false;
        }
        let v = value as f32;
        match rest {
            [param] => {
                match *param {
                    "trim" => self.set_trim(ch, v),
                    "fader" => self.set_fader(ch, v),
                    "filter" => self.set_filter(ch, v),
                    "pfl" => self.set_pfl(ch, v >= 0.5),
                    "xfader" => {
                        let index = value.round().clamp(0.0, 2.0) as u8;
                        self.set_crossfader_assign(ch, assign_from_u8(index));
                    }
                    _ => return false,
                }
                true
            }
            ["eq", band] => {
                let band = match *band {
                    "low" => 0,
                    "mid" => 1,
                    "high" => 2,
                    _ => return false,
                };
                self.set_eq(ch, band, v);
                true
            }
            _ => false,
        }
    }
}

impl ParameterTarget for MixerControl {
    /// Ids: `ch<N>.trim|fader|filter|pfl|xfader`, `ch<N>.eq.low|mid|high`, `crossfader`,
    /// `crossfader.curve` (0 linear, 1 constant power), `master`, `cue.level`, `cue.mix`.
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        let parts: Vec<&str> = id.split('.').collect();
        let v = value as f32;
        match parts.as_slice() {
            [head, rest @ ..] if head.starts_with("ch") && !rest.is_empty() => {
                self.set_channel_parameter(head, rest, value)
            }
            ["crossfader"] => {
                self.set_crossfader(v);
                true
            }
            ["crossfader", "curve"] => {
                self.set_crossfader_curve(if v >= 0.5 {
                    CrossfaderCurve::ConstantPower
                } else {
                    CrossfaderCurve::Linear
                });
                true
            }
            ["master"] => {
                self.set_master_gain(v);
                true
            }
            ["cue", "level"] => {
                self.set_cue_level(v);
                true
            }
            ["cue", "mix"] => {
                self.set_cue_mix(v);
                true
            }
            _ => false,
        }
    }
}
