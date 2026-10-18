//! Beat-synced echo: filtered feedback delay (time in beats).

use super::delay_line::DelayLine;
use super::effect::{Effect, FxContext, ParamSet, ParamSpec, MIN_BPM};
use super::math::flush_denormal;
use om_dsp::{design_high_pass, design_low_pass, Biquad};

// C++ literal, kept verbatim so coefficients stay bit-identical.
#[allow(clippy::approx_constant)]
const Q: f32 = 0.7071;
const LOW_CUT_HZ: f32 = 70.0;
const MIN_TONE_HZ: f32 = 600.0;
pub const MAX_BEATS: f32 = 4.0;

pub const TIME: usize = 0;
pub const FEEDBACK: usize = 1;
pub const TONE: usize = 2;

pub struct EchoFx {
    params: ParamSet,
    line: DelayLine,
    tone_filter: Biquad,
    low_cut: Biquad,
}

impl Default for EchoFx {
    fn default() -> Self {
        Self {
            params: ParamSet::new(&[
                ParamSpec::new("time", 1.0 / 16.0, MAX_BEATS, 0.75).smooth(0.15),
                ParamSpec::new("feedback", 0.0, 0.95, 0.45),
                ParamSpec::new("tone", 0.0, 1.0, 0.6),
            ]),
            line: DelayLine::default(),
            tone_filter: Biquad::default(),
            low_cut: Biquad::default(),
        }
    }
}

impl EchoFx {
    fn update_tone(&mut self, sample_rate: f64) {
        let sr = sample_rate as f32;
        let max_hz = 18000.0_f32.min(sr * 0.45);
        let hz = MIN_TONE_HZ * (max_hz / MIN_TONE_HZ).powf(self.params.value(TONE));
        self.tone_filter.set_coeffs(design_low_pass(sr, hz, Q));
    }
}

impl Effect for EchoFx {
    fn name(&self) -> &'static str {
        "echo"
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
        (f64::from(self.params.target(TIME)) * ctx.samples_per_beat) as usize + 64
    }

    fn prepare_effect(&mut self, sample_rate: f64, _max_block_size: usize) {
        let max_delay = (f64::from(MAX_BEATS) * sample_rate * 60.0 / MIN_BPM).ceil() as usize;
        self.line.allocate(max_delay);
        self.low_cut
            .set_coeffs(design_high_pass(sample_rate as f32, LOW_CUT_HZ, Q));
    }

    fn reset(&mut self) {
        self.line.reset();
        self.tone_filter.reset();
        self.low_cut.reset();
    }

    fn begin_reset(&mut self) {
        self.line.begin_clear();
        self.tone_filter.reset();
        self.low_cut.reset();
    }

    fn clear_some(&mut self, max_frames: usize) -> bool {
        self.line.clear_some(max_frames)
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32], ctx: &FxContext) {
        self.update_tone(ctx.sample_rate);
        let spb = ctx.samples_per_beat as f32;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.params.tick();
            let d = self.params.value(TIME) * spb;
            let fb = self.params.value(FEEDBACK);
            let yl = self.line.read(0, d);
            let yr = self.line.read(1, d);
            let fl = self
                .low_cut
                .process_sample(0, self.tone_filter.process_sample(0, yl));
            let fr = self
                .low_cut
                .process_sample(1, self.tone_filter.process_sample(1, yr));
            self.line
                .push(flush_denormal(*l + fb * fl), flush_denormal(*r + fb * fr));
            *l = yl;
            *r = yr;
        }
    }
}
