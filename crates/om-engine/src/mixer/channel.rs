//! One mixer channel, audio side: trim, EQ, sweep filter, inserts, PFL tap, fader and crossfader gain.

use super::shared::{assign_from_u8, MixerShared};
use crate::stage::StageChain;
use om_dsp::{Crossfader, CrossfaderGains, SweepFilter, ThreeBandEq};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Output buffers for one chunk. All slices have the same length.
pub struct Bus<'a> {
    pub master_l: &'a mut [f32],
    pub master_r: &'a mut [f32],
    pub cue_l: &'a mut [f32],
    pub cue_r: &'a mut [f32],
}

impl Bus<'_> {
    pub fn clear(&mut self) {
        self.master_l.fill(0.0);
        self.master_r.fill(0.0);
        self.cue_l.fill(0.0);
        self.cue_r.fill(0.0);
    }
}

pub struct ChannelStrip {
    shared: Arc<MixerShared>,
    index: usize,
    eq: ThreeBandEq,
    filter: SweepFilter,
    inserts: StageChain,
    applied_eq: [f32; 3],
    applied_filter: f32,
    trim_gain: f32,
    fader_gain: f32, // negative: snap to the target on the first block
    scratch_l: Vec<f32>,
    scratch_r: Vec<f32>,
}

impl ChannelStrip {
    pub fn new(shared: Arc<MixerShared>, index: usize, inserts: StageChain) -> Self {
        Self {
            shared,
            index,
            eq: ThreeBandEq::new(),
            filter: SweepFilter::new(),
            inserts,
            applied_eq: [0.0; 3],
            applied_filter: 0.0,
            trim_gain: 1.0,
            fader_gain: -1.0,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    pub fn prepare(&mut self, sample_rate: f64, max_block: usize) {
        self.scratch_l = vec![0.0; max_block];
        self.scratch_r = vec![0.0; max_block];
        self.eq.prepare(sample_rate);
        self.filter.prepare(sample_rate);
        self.inserts.prepare(sample_rate, max_block);
        let cells = &self.shared.channels[self.index];
        self.trim_gain = cells.trim.load();
        self.fader_gain = -1.0;
        cells.meter_left.reset();
        cells.meter_right.reset();
    }

    /// Copies control-thread EQ and filter values into the DSP units when they have changed.
    fn sync_dsp(&mut self) {
        let cells = &self.shared.channels[self.index];
        for (band, cell) in cells.eq.iter().enumerate() {
            let value = cell.load();
            if value != self.applied_eq[band] {
                self.eq.set_gain_db(band, value);
                self.applied_eq[band] = value;
            }
        }
        let position = cells.filter.load();
        if position != self.applied_filter {
            self.filter.set_position(position);
            self.applied_filter = position;
        }
    }

    fn apply_trim(&mut self, src_l: &[f32], src_r: &[f32], n: usize) {
        let target = self.shared.channels[self.index].trim.load();
        let step = (target - self.trim_gain) / n as f32;
        let mut g = self.trim_gain;
        for i in 0..n {
            self.scratch_l[i] = src_l[i] * g;
            self.scratch_r[i] = src_r[i] * g;
            g += step;
        }
        self.trim_gain = target;
    }

    /// Adds this channel into the master bus with a fader ramp; returns the block's stereo peak.
    fn mix_into_master(&mut self, target: f32, bus: &mut Bus<'_>, n: usize) -> (f32, f32) {
        let step = (target - self.fader_gain) / n as f32;
        let mut g = self.fader_gain;
        let (mut peak_l, mut peak_r) = (0.0_f32, 0.0_f32);
        for i in 0..n {
            let l = self.scratch_l[i] * g;
            let r = self.scratch_r[i] * g;
            bus.master_l[i] += l;
            bus.master_r[i] += r;
            peak_l = peak_l.max(l.abs());
            peak_r = peak_r.max(r.abs());
            g += step;
        }
        self.fader_gain = target;
        (peak_l, peak_r)
    }

    /// Renders one chunk of `n` frames. `None` means silence; a missing side copies the other.
    pub fn render(
        &mut self,
        src_l: Option<&[f32]>,
        src_r: Option<&[f32]>,
        xf: CrossfaderGains,
        bus: &mut Bus<'_>,
        n: usize,
        decay: f32,
    ) {
        let cells = &self.shared.channels[self.index];
        let target = cells.fader.load()
            * Crossfader::gain_for(xf, assign_from_u8(cells.assign.load(Ordering::Relaxed)));
        if self.fader_gain < 0.0 {
            self.fader_gain = target;
        }

        let (src_l, src_r) = match (src_l, src_r) {
            (None, None) => {
                self.fader_gain = target;
                self.trim_gain = cells.trim.load();
                cells.meter_left.push(0.0, decay);
                cells.meter_right.push(0.0, decay);
                return;
            }
            (Some(l), Some(r)) => (l, r),
            (Some(l), None) => (l, l),
            (None, Some(r)) => (r, r),
        };

        self.apply_trim(&src_l[..n], &src_r[..n], n);
        self.sync_dsp();
        self.eq
            .process(&mut self.scratch_l[..n], &mut self.scratch_r[..n]);
        self.filter
            .process(&mut self.scratch_l[..n], &mut self.scratch_r[..n]);
        self.inserts
            .process(&mut self.scratch_l[..n], &mut self.scratch_r[..n]);

        if self.shared.channels[self.index].pfl.load(Ordering::Relaxed) {
            for i in 0..n {
                bus.cue_l[i] += self.scratch_l[i];
                bus.cue_r[i] += self.scratch_r[i];
            }
        }
        let (peak_l, peak_r) = self.mix_into_master(target, bus, n);
        let cells = &self.shared.channels[self.index];
        cells.meter_left.push(peak_l, decay);
        cells.meter_right.push(peak_r, decay);
    }
}
