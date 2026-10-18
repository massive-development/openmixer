//! Stereo room reverb: parallel damped combs into series allpasses (Schroeder/Moorer topology).

use super::effect::{Effect, FxContext, ParamSet, ParamSpec};
use super::reverb_parts::{DampedComb, SchroederAllpass};

pub const SIZE: usize = 0;
pub const DAMP: usize = 1;
pub const WIDTH: usize = 2;

pub const NUM_COMBS: usize = 8;
pub const NUM_ALLPASSES: usize = 4;

// Mutually prime lengths (samples at 44.1 kHz), chosen for OpenMixer; right side is offset.
const COMB_LENGTHS: [usize; NUM_COMBS] = [1097, 1171, 1259, 1327, 1409, 1483, 1549, 1613];
const ALLPASS_LENGTHS: [usize; NUM_ALLPASSES] = [563, 433, 337, 241];
const STEREO_OFFSET: usize = 29;
const INPUT_GAIN: f32 = 0.02;
const REFERENCE_RATE: f64 = 44100.0;

pub struct ReverbFx {
    params: ParamSet,
    combs: [[DampedComb; NUM_COMBS]; 2],
    allpasses: [[SchroederAllpass; NUM_ALLPASSES]; 2],
}

impl Default for ReverbFx {
    fn default() -> Self {
        Self {
            params: ParamSet::new(&[
                ParamSpec::new("size", 0.0, 1.0, 0.7),
                ParamSpec::new("damp", 0.0, 1.0, 0.4),
                ParamSpec::new("width", 0.0, 1.0, 1.0),
            ]),
            combs: Default::default(),
            allpasses: Default::default(),
        }
    }
}

impl ReverbFx {
    fn process_channel(&mut self, channel: usize, input: f32, feedback: f32, damp: f32) -> f32 {
        let mut sum = 0.0_f32;
        for comb in &mut self.combs[channel] {
            sum += comb.process(input, feedback, damp);
        }
        for ap in &mut self.allpasses[channel] {
            sum = ap.process(sum);
        }
        sum
    }
}

impl Effect for ReverbFx {
    fn name(&self) -> &'static str {
        "reverb"
    }

    fn params(&self) -> &ParamSet {
        &self.params
    }

    fn params_mut(&mut self) -> &mut ParamSet {
        &mut self.params
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn tail_silence_window(&self, ctx: &FxContext) -> usize {
        (ctx.sample_rate * 0.1) as usize
    }

    fn prepare_effect(&mut self, sample_rate: f64, _max_block_size: usize) {
        let scale = sample_rate / REFERENCE_RATE;
        for (c, (combs, allpasses)) in self.combs.iter_mut().zip(&mut self.allpasses).enumerate() {
            let offset = if c == 0 { 0 } else { STEREO_OFFSET };
            for (i, comb) in combs.iter_mut().enumerate() {
                comb.allocate(((COMB_LENGTHS[i] + offset) as f64 * scale) as usize);
            }
            for (i, ap) in allpasses.iter_mut().enumerate() {
                ap.allocate(((ALLPASS_LENGTHS[i] + offset) as f64 * scale) as usize);
            }
        }
    }

    fn reset(&mut self) {
        for comb in self.combs.iter_mut().flatten() {
            comb.reset();
        }
        for ap in self.allpasses.iter_mut().flatten() {
            ap.reset();
        }
    }

    fn begin_reset(&mut self) {
        for comb in self.combs.iter_mut().flatten() {
            comb.begin_clear();
        }
        for ap in self.allpasses.iter_mut().flatten() {
            ap.begin_clear();
        }
    }

    fn clear_some(&mut self, max_frames: usize) -> bool {
        // One budget shared by all 24 buffers keeps the per-call cost bounded.
        let mut budget = max_frames;
        let mut clean = true;
        for comb in self.combs.iter_mut().flatten() {
            budget -= comb.clear_some(budget);
            clean = clean && comb.clean();
        }
        for ap in self.allpasses.iter_mut().flatten() {
            budget -= ap.clear_some(budget);
            clean = clean && ap.clean();
        }
        clean
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32], _ctx: &FxContext) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.params.tick();
            let feedback = 0.7 + 0.27 * self.params.value(SIZE);
            let damp = 0.05 + 0.6 * self.params.value(DAMP);
            let input = (*l + *r) * INPUT_GAIN;
            let wl = self.process_channel(0, input, feedback, damp);
            let wr = self.process_channel(1, input, feedback, damp);
            let w = self.params.value(WIDTH);
            let mid = 0.5 * (wl + wr) * (1.0 - w);
            *l = wl * w + mid;
            *r = wr * w + mid;
        }
    }
}
