//! Phaser: six swept first-order allpass stages with feedback; sweep period in beats.

use super::effect::{Effect, FxContext, ParamSet, ParamSpec};
use super::math::flush_denormal;

const LOW_HZ: f64 = 250.0;
const HIGH_HZ: f64 = 4000.0;
// Literal kept verbatim so the sweep stays bit-identical.
#[allow(clippy::approx_constant)]
const PI: f64 = 3.141_592_653_589_793;

pub const TIME: usize = 0;
pub const DEPTH: usize = 1;
pub const FEEDBACK: usize = 2;
pub const STAGES: usize = 6;
/// Entries in the sweep-to-coefficient table (plus one so the last cell can interpolate).
const TABLE_CELLS: usize = 512;

pub struct PhaserFx {
    params: ParamSet,
    state: [[f32; STAGES]; 2],
    last: [f32; 2],
    phase: f64,
    /// All-pass coefficient at `sweep = i / TABLE_CELLS`; the tan/powf per sample become a lerp.
    table: [f32; TABLE_CELLS + 1],
    table_rate: f64,
}

impl Default for PhaserFx {
    fn default() -> Self {
        Self {
            params: ParamSet::new(&[
                ParamSpec::new("time", 0.25, 32.0, 4.0).smooth(0.0),
                ParamSpec::new("depth", 0.0, 1.0, 0.8),
                ParamSpec::new("feedback", 0.0, 0.9, 0.4),
            ]),
            state: [[0.0; STAGES]; 2],
            last: [0.0; 2],
            phase: 0.0,
            table: [0.0; TABLE_CELLS + 1],
            table_rate: 0.0,
        }
    }
}

impl PhaserFx {
    fn exact_coefficient(sweep: f64, sample_rate: f64) -> f32 {
        let hz = (LOW_HZ * (HIGH_HZ / LOW_HZ).powf(sweep)).min(sample_rate * 0.45);
        let t = (PI * hz / sample_rate).tan();
        ((t - 1.0) / (t + 1.0)) as f32
    }

    fn build_table(&mut self, sample_rate: f64) {
        for (i, a) in self.table.iter_mut().enumerate() {
            *a = Self::exact_coefficient(i as f64 / TABLE_CELLS as f64, sample_rate);
        }
        self.table_rate = sample_rate;
    }

    /// Coefficient for `sweep` in [0, 1], interpolated from the table.
    fn coefficient_for(&self, sweep: f64) -> f32 {
        let x = sweep.clamp(0.0, 1.0) * TABLE_CELLS as f64;
        let i = (x as usize).min(TABLE_CELLS - 1);
        let frac = (x - i as f64) as f32;
        let (lo, hi) = (self.table[i], self.table[i + 1]);
        lo + frac * (hi - lo)
    }

    fn run_stages(&mut self, channel: usize, input: f32, a: f32) -> f32 {
        let mut x = input;
        for s in &mut self.state[channel] {
            let y = a * x + *s;
            *s = flush_denormal(x - a * y);
            x = y;
        }
        x
    }
}

impl Effect for PhaserFx {
    fn name(&self) -> &'static str {
        "phaser"
    }

    fn params(&self) -> &ParamSet {
        &self.params
    }

    fn params_mut(&mut self) -> &mut ParamSet {
        &mut self.params
    }

    fn prepare_effect(&mut self, sample_rate: f64, _max_block_size: usize) {
        self.build_table(sample_rate);
    }

    fn reset(&mut self) {
        for ch in &mut self.state {
            ch.fill(0.0);
        }
        self.last.fill(0.0);
        self.phase = 0.0;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32], ctx: &FxContext) {
        if self.table_rate != ctx.sample_rate {
            self.build_table(ctx.sample_rate);
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.params.tick();
            self.phase += 1.0 / (f64::from(self.params.value(TIME)) * ctx.samples_per_beat);
            if self.phase >= 1.0 {
                self.phase -= self.phase.floor();
            }
            let fb = self.params.value(FEEDBACK);
            let in_gain = 1.0 - fb;
            // The right channel runs a quarter cycle ahead: cos(x + pi/2) = -sin(x).
            let (sin, cos) = (2.0 * PI * self.phase).sin_cos();
            let depth = f64::from(self.params.value(DEPTH));
            let sweeps = [(0.5 - 0.5 * cos) * depth, (0.5 + 0.5 * sin) * depth];
            for (c, out) in [l, r].into_iter().enumerate() {
                let a = self.coefficient_for(sweeps[c]);
                self.last[c] = self.run_stages(c, in_gain * *out + fb * self.last[c], a);
                *out = 0.5 * (*out + self.last[c]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_coefficient_tracks_the_exact_formula() {
        let mut fx = PhaserFx::default();
        for rate in [44_100.0, 48_000.0, 96_000.0] {
            fx.build_table(rate);
            for i in 0..=2000 {
                let sweep = f64::from(i) / 2000.0;
                let exact = PhaserFx::exact_coefficient(sweep, rate);
                assert!(
                    (fx.coefficient_for(sweep) - exact).abs() < 2e-5,
                    "{rate} {sweep}"
                );
            }
        }
    }
}
