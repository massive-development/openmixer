//! Building blocks for the reverb: damped feedback comb and Schroeder allpass (textbook forms).

use super::math::flush_denormal;
use super::wipe::Wipe;

/// Zeroes up to `max_frames` stale slots of `buffer`; returns how many were used.
fn wipe_slots(buffer: &mut [f32], wipe: &mut Wipe, max_frames: usize) -> usize {
    let (from, count) = wipe.claim(buffer.len(), max_frames);
    buffer[from..from + count].fill(0.0);
    count
}

#[derive(Debug, Default)]
pub struct DampedComb {
    buffer: Vec<f32>,
    wipe: Wipe,
    pos: usize,
    low_state: f32,
}

impl DampedComb {
    pub fn allocate(&mut self, length: usize) {
        self.buffer = vec![0.0; length.max(1)];
        self.pos = 0;
        self.wipe.finish(self.buffer.len());
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
        self.low_state = 0.0;
        self.wipe.finish(self.buffer.len());
    }

    pub fn begin_clear(&mut self) {
        self.pos = 0;
        self.low_state = 0.0;
        self.wipe.begin();
    }

    pub fn clear_some(&mut self, max_frames: usize) -> usize {
        wipe_slots(&mut self.buffer, &mut self.wipe, max_frames)
    }

    pub fn clean(&self) -> bool {
        self.wipe.done(self.buffer.len())
    }

    pub fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let i = self.pos;
        let out = if self.wipe.live(i) {
            self.buffer[i]
        } else {
            0.0
        };
        self.low_state = flush_denormal(out + damp * (self.low_state - out));
        self.buffer[i] = flush_denormal(input + feedback * self.low_state);
        self.wipe.wrote(i);
        self.pos = if i + 1 == self.buffer.len() { 0 } else { i + 1 };
        out
    }
}

#[derive(Debug, Default)]
pub struct SchroederAllpass {
    buffer: Vec<f32>,
    wipe: Wipe,
    pos: usize,
}

impl SchroederAllpass {
    const G: f32 = 0.5;

    pub fn allocate(&mut self, length: usize) {
        self.buffer = vec![0.0; length.max(1)];
        self.pos = 0;
        self.wipe.finish(self.buffer.len());
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
        self.wipe.finish(self.buffer.len());
    }

    pub fn begin_clear(&mut self) {
        self.pos = 0;
        self.wipe.begin();
    }

    pub fn clear_some(&mut self, max_frames: usize) -> usize {
        wipe_slots(&mut self.buffer, &mut self.wipe, max_frames)
    }

    pub fn clean(&self) -> bool {
        self.wipe.done(self.buffer.len())
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let i = self.pos;
        let delayed = if self.wipe.live(i) {
            self.buffer[i]
        } else {
            0.0
        };
        self.buffer[i] = flush_denormal(input + Self::G * delayed);
        self.wipe.wrote(i);
        self.pos = if i + 1 == self.buffer.len() { 0 } else { i + 1 };
        delayed - Self::G * self.buffer[i]
    }
}
