//! Flanger: short modulated delay with feedback; the sweep period is in beats.

use super::delay_line::DelayLine;
use super::effect::{Effect, FxContext, ParamSet, ParamSpec};
use super::math::flush_denormal;

const MIN_DELAY_SEC: f64 = 0.0005;
const SWEEP_SEC: f64 = 0.004;
// C++ literal, kept verbatim so the sweep stays bit-identical.
#[allow(clippy::approx_constant)]
const TWO_PI: f64 = 6.283_185_307_179_586;

pub const TIME: usize = 0;
pub const DEPTH: usize = 1;
pub const FEEDBACK: usize = 2;

pub struct FlangerFx {
    params: ParamSet,
    line: DelayLine,
    phase: f64,
    last_l: f32,
    last_r: f32,
}

impl Default for FlangerFx {
    fn default() -> Self {
        Self {
            params: ParamSet::new(&[
                ParamSpec::new("time", 0.25, 32.0, 8.0).smooth(0.0),
                ParamSpec::new("depth", 0.0, 1.0, 0.7),
                ParamSpec::new("feedback", -0.9, 0.9, 0.5),
            ]),
            line: DelayLine::default(),
            phase: 0.0,
            last_l: 0.0,
            last_r: 0.0,
        }
    }
}

impl FlangerFx {
    fn delay_for(&self, ph: f64, sample_rate: f64) -> f32 {
        let sweep = 0.5 - 0.5 * (TWO_PI * ph).cos();
        ((MIN_DELAY_SEC + SWEEP_SEC * f64::from(self.params.value(DEPTH)) * sweep) * sample_rate)
            as f32
    }
}

impl Effect for FlangerFx {
    fn name(&self) -> &'static str {
        "flanger"
    }

    fn params(&self) -> &ParamSet {
        &self.params
    }

    fn params_mut(&mut self) -> &mut ParamSet {
        &mut self.params
    }

    fn prepare_effect(&mut self, sample_rate: f64, _max_block_size: usize) {
        self.line
            .allocate(((MIN_DELAY_SEC + SWEEP_SEC) * sample_rate).ceil() as usize + 4);
    }

    fn reset(&mut self) {
        self.line.reset();
        self.phase = 0.0;
        self.last_l = 0.0;
        self.last_r = 0.0;
    }

    fn begin_reset(&mut self) {
        self.line.begin_clear();
        self.phase = 0.0;
        self.last_l = 0.0;
        self.last_r = 0.0;
    }

    fn clear_some(&mut self, max_frames: usize) -> bool {
        self.line.clear_some(max_frames)
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32], ctx: &FxContext) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.params.tick();
            self.phase += 1.0 / (f64::from(self.params.value(TIME)) * ctx.samples_per_beat);
            self.phase -= self.phase.floor();
            // Input scaled by (1 - |fb|) caps the resonant peak at unity gain.
            let fb = self.params.value(FEEDBACK);
            let in_gain = 1.0 - fb.abs();
            self.line.push(
                flush_denormal(in_gain * *l + fb * self.last_l),
                flush_denormal(in_gain * *r + fb * self.last_r),
            );
            self.last_l = self
                .line
                .read(0, self.delay_for(self.phase, ctx.sample_rate));
            self.last_r = self
                .line
                .read(1, self.delay_for(self.phase + 0.25, ctx.sample_rate));
            *l = 0.5 * (*l + self.last_l);
            *r = 0.5 * (*r + self.last_r);
        }
    }
}
