//! Beat roll: captures one beat-division of audio when engaged and repeats it.

use super::effect::{Effect, FxContext, ParamSet, ParamSpec, MIN_BPM};

use std::cmp::min;

const EDGE_FADE: usize = 96;
pub const MAX_BEATS: f32 = 4.0;

pub const TIME: usize = 0;

pub struct RollFx {
    params: ParamSet,
    loop_buf: [Vec<f32>; 2],
    capacity: usize,
    recorded: usize,
    play_pos: usize,
}

impl Default for RollFx {
    fn default() -> Self {
        Self {
            params: ParamSet::new(
                &[ParamSpec::new("time", 1.0 / 16.0, MAX_BEATS, 0.5).smooth(0.0)],
            ),
            loop_buf: [Vec::new(), Vec::new()],
            capacity: 0,
            recorded: 0,
            play_pos: 0,
        }
    }
}

impl RollFx {
    /// Short fades at the loop seam avoid a click on every repeat.
    fn edge_gain(pos: usize, length: usize, fade_in: bool) -> f32 {
        let fade = min(EDGE_FADE, length / 4);
        if fade == 0 {
            return 1.0;
        }
        let edge = if fade_in {
            min(pos, length - 1 - pos)
        } else {
            length - 1 - pos
        };
        1.0_f32.min(edge as f32 / fade as f32)
    }
}

impl Effect for RollFx {
    fn name(&self) -> &'static str {
        "roll"
    }

    fn params(&self) -> &ParamSet {
        &self.params
    }

    fn params_mut(&mut self) -> &mut ParamSet {
        &mut self.params
    }

    fn prepare_effect(&mut self, sample_rate: f64, _max_block_size: usize) {
        self.capacity = (f64::from(MAX_BEATS) * sample_rate * 60.0 / MIN_BPM).ceil() as usize;
        for ch in &mut self.loop_buf {
            *ch = vec![0.0; self.capacity];
        }
    }

    /// Engaging restarts the capture. Reset only rewinds counters (reads never pass `recorded`),
    /// so it is O(1) on the audio thread and needs no sliced clear.
    fn reset(&mut self) {
        self.recorded = 0;
        self.play_pos = 0;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32], ctx: &FxContext) {
        if self.capacity == 0 {
            return;
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.params.tick();
            let length = ((f64::from(self.params.value(TIME)) * ctx.samples_per_beat) as usize)
                .clamp(1, self.capacity);
            if self.recorded < length {
                self.loop_buf[0][self.recorded] = *l;
                self.loop_buf[1][self.recorded] = *r;
                let g = Self::edge_gain(self.recorded, length, false); // live pass: fade out only
                *l *= g;
                *r *= g;
                self.recorded += 1;
                continue;
            }
            self.play_pos %= length;
            let g = Self::edge_gain(self.play_pos, length, true);
            *l = self.loop_buf[0][self.play_pos] * g;
            *r = self.loop_buf[1][self.play_pos] * g;
            self.play_pos += 1;
        }
    }
}
