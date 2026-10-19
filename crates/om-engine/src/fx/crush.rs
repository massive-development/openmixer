//! Bit crusher: amplitude quantisation plus sample-rate reduction (sample-and-hold).

use super::effect::{Effect, FxContext, ParamSet, ParamSpec};

pub const BITS: usize = 0;
pub const DOWNSAMPLE: usize = 1;

pub struct CrushFx {
    params: ParamSet,
    held: [f32; 2],
    hold_phase: f32,
}

impl Default for CrushFx {
    fn default() -> Self {
        Self {
            params: ParamSet::new(&[
                ParamSpec::new("bits", 2.0, 16.0, 8.0),
                ParamSpec::new("downsample", 1.0, 32.0, 4.0),
            ]),
            held: [0.0; 2],
            hold_phase: 1.0,
        }
    }
}

impl Effect for CrushFx {
    fn name(&self) -> &'static str {
        "crush"
    }

    fn params(&self) -> &ParamSet {
        &self.params
    }

    fn params_mut(&mut self) -> &mut ParamSet {
        &mut self.params
    }

    fn reset(&mut self) {
        self.held = [0.0; 2];
        self.hold_phase = 1.0;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32], _ctx: &FxContext) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.params.tick();
            self.hold_phase += 1.0 / self.params.value(DOWNSAMPLE);
            if self.hold_phase >= 1.0 {
                self.hold_phase -= self.hold_phase.floor();
                let step = (1.0 - self.params.value(BITS)).exp2();
                self.held[0] = (*l / step).round() * step;
                self.held[1] = (*r / step).round() * step;
            }
            *l = self.held[0];
            *r = self.held[1];
        }
    }
}
