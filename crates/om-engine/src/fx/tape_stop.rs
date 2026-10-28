//! Tape stop: playback speed (and pitch) ramps from 1 to 0 over a beat-synced time.

use super::effect::{Effect, FxContext, ParamSet, ParamSpec, MIN_BPM};

const SILENCE_BELOW_SPEED: f64 = 0.125; // fade to silence over the last part of the stop
pub const MAX_BEATS: f32 = 8.0;

pub const TIME: usize = 0;

pub struct TapeStopFx {
    params: ParamSet,
    history: [Vec<f32>; 2],
    written: i64,
    /// History slot the next frame is written to; always `written % size`, kept incrementally so
    /// the per-sample path never divides.
    head: usize,
    read_pos: f64,
    speed: f64,
}

impl Default for TapeStopFx {
    fn default() -> Self {
        Self {
            params: ParamSet::new(&[ParamSpec::new("time", 0.25, MAX_BEATS, 1.0).smooth(0.0)]),
            history: [Vec::new(), Vec::new()],
            written: 0,
            head: 0,
            read_pos: 0.0,
            speed: 1.0,
        }
    }
}

impl TapeStopFx {
    /// Slot of frame `index` (`< written`): `lag` frames behind `head`, wrapped with one add. A lag
    /// beyond the buffer is outside the design and falls back to the exact modulo.
    fn slot(&self, index: i64, size: i64) -> usize {
        let s = self.head as i64 - (self.written - index);
        (if s >= 0 {
            s
        } else if s >= -size {
            s + size
        } else {
            s.rem_euclid(size)
        }) as usize
    }

    fn read_at(&self, channel: usize, position: f64) -> f32 {
        let h = &self.history[channel];
        let size = h.len() as i64;
        let whole = position.floor() as i64;
        let frac = (position - whole as f64) as f32;
        let next = (whole + 1).min(self.written - 1);
        let a = h[self.slot(whole, size)];
        let b = h[self.slot(next, size)];
        a + frac * (b - a)
    }
}

impl Effect for TapeStopFx {
    fn name(&self) -> &'static str {
        "tapestop"
    }

    fn params(&self) -> &ParamSet {
        &self.params
    }

    fn params_mut(&mut self) -> &mut ParamSet {
        &mut self.params
    }

    fn prepare_effect(&mut self, sample_rate: f64, _max_block_size: usize) {
        // The read head lags the write head by at most the stop length.
        let size = (f64::from(MAX_BEATS) * sample_rate * 60.0 / MIN_BPM).ceil() as usize + 4;
        for ch in &mut self.history {
            *ch = vec![0.0; size];
        }
    }

    /// Engaging restarts the slow-down from full speed. Reset only rewinds counters (reads never
    /// pass `written`), so it is O(1) on the audio thread and needs no sliced clear.
    fn reset(&mut self) {
        self.written = 0;
        self.head = 0;
        self.read_pos = 0.0;
        self.speed = 1.0;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32], ctx: &FxContext) {
        if self.history[0].is_empty() {
            return;
        }
        let size = self.history[0].len();
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.params.tick();
            self.history[0][self.head] = *l;
            self.history[1][self.head] = *r;
            self.written += 1;
            self.head = if self.head + 1 == size {
                0
            } else {
                self.head + 1
            };
            let gain = (1.0_f64).min(self.speed / SILENCE_BELOW_SPEED) as f32;
            *l = self.read_at(0, self.read_pos) * gain;
            *r = self.read_at(1, self.read_pos) * gain;
            self.read_pos += self.speed;
            self.speed = (self.speed
                - 1.0 / (f64::from(self.params.value(TIME)) * ctx.samples_per_beat))
                .max(0.0);
        }
    }
}
