//! Trance gate: rhythmic on/off chopping at a beat division, with click-free edges.

use super::effect::{Effect, FxContext, ParamSet, ParamSpec};
use super::math::flush_denormal;

const ATTACK_SEC: f64 = 0.001;
const RELEASE_SEC: f64 = 0.004;

pub const TIME: usize = 0;
pub const DUTY: usize = 1;

fn one_pole_coeff(seconds: f64, sample_rate: f64) -> f32 {
    (1.0 - (-1.0 / (seconds * sample_rate)).exp()) as f32
}

pub struct GateFx {
    params: ParamSet,
    phase: f64,
    envelope: f32,
    attack: f32,
    release: f32,
}

impl Default for GateFx {
    fn default() -> Self {
        Self {
            params: ParamSet::new(&[
                ParamSpec::new("time", 1.0 / 16.0, 2.0, 0.25).smooth(0.0),
                ParamSpec::new("duty", 0.1, 0.9, 0.5),
            ]),
            phase: 0.0,
            envelope: 1.0,
            attack: 1.0,
            release: 1.0,
        }
    }
}

impl Effect for GateFx {
    fn name(&self) -> &'static str {
        "gate"
    }

    fn params(&self) -> &ParamSet {
        &self.params
    }

    fn params_mut(&mut self) -> &mut ParamSet {
        &mut self.params
    }

    fn prepare_effect(&mut self, sample_rate: f64, _max_block_size: usize) {
        self.attack = one_pole_coeff(ATTACK_SEC, sample_rate);
        self.release = one_pole_coeff(RELEASE_SEC, sample_rate);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.envelope = 1.0;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32], ctx: &FxContext) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.params.tick();
            let open = if self.phase < f64::from(self.params.value(DUTY)) {
                1.0_f32
            } else {
                0.0
            };
            let coeff = if open > self.envelope {
                self.attack
            } else {
                self.release
            };
            self.envelope = flush_denormal(self.envelope + coeff * (open - self.envelope));
            self.phase += 1.0 / (f64::from(self.params.value(TIME)) * ctx.samples_per_beat);
            self.phase -= self.phase.floor();
            *l *= self.envelope;
            *r *= self.envelope;
        }
    }
}
