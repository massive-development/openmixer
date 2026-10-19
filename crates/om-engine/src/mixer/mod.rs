//! Mixer: channel strips, crossfader, master bus and PFL cue bus.
//!
//! [`Mixer::new`] returns the audio-side owner and a [`MixerControl`]. The control handle is the
//! shared entry point for MIDI, UI and tests: it writes atomics that the audio side reads at the
//! start of each chunk, and it queues insert changes. Threads: `prepare` on the control thread;
//! `process` on the audio thread; `MixerControl` from any non-audio thread.

mod channel;
mod control;
mod shared;

use crate::stage::StageChain;
use channel::{Bus, ChannelStrip};
use om_dsp::{Crossfader, CrossfaderCurve};
use shared::MixerShared;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub use control::MixerControl;
pub use shared::{MAX_CHANNELS, MAX_GAIN, MAX_TRIM};

/// Meter decay time constant in seconds (C++ `kMeterDecaySeconds`).
const METER_DECAY_SECONDS: f32 = 0.3;
const DEFAULT_SAMPLE_RATE: f64 = 44_100.0;

/// One channel's input. `None` on a side means silence; a missing right side copies the left.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelInput<'a> {
    pub left: Option<&'a [f32]>,
    pub right: Option<&'a [f32]>,
}

pub struct Mixer {
    shared: Arc<MixerShared>,
    channels: Vec<ChannelStrip>,
    crossfader: Crossfader,
    applied_crossfader: f32,
    applied_curve: CrossfaderCurve,
    master_inserts: StageChain,
    sample_rate: f64,
    max_block: usize,
    prepared: bool,
}

impl Mixer {
    /// Builds the mixer with `num_channels` clamped to `1..=MAX_CHANNELS`. Control thread.
    pub fn new(num_channels: usize) -> (Self, MixerControl) {
        let n = num_channels.clamp(1, MAX_CHANNELS);
        let shared = Arc::new(MixerShared::new(n));
        let mut channels = Vec::with_capacity(n);
        let mut channel_inserts = Vec::with_capacity(n);
        for index in 0..n {
            let (inserts, insert) = StageChain::new();
            channels.push(ChannelStrip::new(Arc::clone(&shared), index, inserts));
            channel_inserts.push(insert);
        }
        let (master_inserts, master_insert) = StageChain::new();
        let crossfader = Crossfader::default();
        let mixer = Self {
            shared: Arc::clone(&shared),
            channels,
            applied_crossfader: crossfader.position(),
            applied_curve: crossfader.curve(),
            crossfader,
            master_inserts,
            sample_rate: DEFAULT_SAMPLE_RATE,
            max_block: 0,
            prepared: false,
        };
        let control = MixerControl::new(shared, channel_inserts, master_insert);
        (mixer, control)
    }

    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }

    /// Control thread, before audio runs. Sizes every buffer for `max_block_size` frames.
    pub fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        self.sample_rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            DEFAULT_SAMPLE_RATE
        };
        self.max_block = max_block_size.max(1);
        for channel in &mut self.channels {
            channel.prepare(self.sample_rate, self.max_block);
        }
        self.master_inserts
            .prepare(self.sample_rate, self.max_block);
        self.shared.master_peak_left.reset();
        self.shared.master_peak_right.reset();
        self.prepared = true;
    }

    /// Audio thread. Renders `master_l.len()` frames into the four outputs, which are overwritten.
    /// `inputs` holds one entry per channel; missing entries are silent. Before `prepare` the
    /// outputs are silenced.
    pub fn process(
        &mut self,
        inputs: &[ChannelInput<'_>],
        master_l: &mut [f32],
        master_r: &mut [f32],
        cue_l: &mut [f32],
        cue_r: &mut [f32],
    ) {
        let total = master_l.len();
        assert!(
            master_r.len() == total && cue_l.len() == total && cue_r.len() == total,
            "mixer outputs must have equal length"
        );
        if !self.prepared {
            for out in [master_l, master_r, cue_l, cue_r] {
                out.fill(0.0);
            }
            return;
        }
        let num = self.channels.len();
        let mut done = 0;
        while done < total {
            let n = self.max_block.min(total - done);
            let end = done + n;
            let mut shifted = [ChannelInput::default(); MAX_CHANNELS];
            for (c, slot) in shifted.iter_mut().enumerate().take(num) {
                if let Some(input) = inputs.get(c) {
                    slot.left = input.left.map(|s| &s[done..end]);
                    slot.right = input.right.map(|s| &s[done..end]);
                }
            }
            let mut bus = Bus {
                master_l: &mut master_l[done..end],
                master_r: &mut master_r[done..end],
                cue_l: &mut cue_l[done..end],
                cue_r: &mut cue_r[done..end],
            };
            self.process_chunk(&shifted[..num], &mut bus, n);
            done = end;
        }
    }

    fn process_chunk(&mut self, inputs: &[ChannelInput<'_>], bus: &mut Bus<'_>, n: usize) {
        bus.clear();
        let decay = (-(n as f32) / (self.sample_rate as f32 * METER_DECAY_SECONDS)).exp();
        self.sync_crossfader();
        let xf = self.crossfader.gains();
        for (channel, input) in self.channels.iter_mut().zip(inputs) {
            channel.render(input.left, input.right, xf, bus, n, decay);
        }
        self.finish_master(bus, n, decay);
        self.mix_headphones(bus, n);
    }

    /// Copies control-thread crossfader values into the DSP unit when they have changed.
    fn sync_crossfader(&mut self) {
        let position = self.shared.crossfader.load();
        if position != self.applied_crossfader {
            self.crossfader.set_position(position);
            self.applied_crossfader = position;
        }
        let curve = if self.shared.constant_power.load(Ordering::Relaxed) {
            CrossfaderCurve::ConstantPower
        } else {
            CrossfaderCurve::Linear
        };
        if curve != self.applied_curve {
            self.crossfader.set_curve(curve);
            self.applied_curve = curve;
        }
    }

    fn finish_master(&mut self, bus: &mut Bus<'_>, n: usize, decay: f32) {
        let gain = self.shared.master_gain.load();
        for i in 0..n {
            bus.master_l[i] *= gain;
            bus.master_r[i] *= gain;
        }
        self.master_inserts.process(bus.master_l, bus.master_r);

        let (mut peak_l, mut peak_r) = (0.0_f32, 0.0_f32);
        for i in 0..n {
            peak_l = peak_l.max(bus.master_l[i].abs());
            peak_r = peak_r.max(bus.master_r[i].abs());
        }
        self.shared.master_peak_left.push(peak_l, decay);
        self.shared.master_peak_right.push(peak_r, decay);
    }

    fn mix_headphones(&self, bus: &mut Bus<'_>, n: usize) {
        let level = self.shared.cue_level.load();
        let mix = self.shared.cue_mix.load();
        for i in 0..n {
            bus.cue_l[i] = level * ((1.0 - mix) * bus.cue_l[i] + mix * bus.master_l[i]);
            bus.cue_r[i] = level * ((1.0 - mix) * bus.cue_r[i] + mix * bus.master_r[i]);
        }
    }
}
