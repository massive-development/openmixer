//! Stereo circular buffer with fractional (linear) reads. Sized on the control thread.

use super::wipe::Wipe;

#[derive(Debug, Default)]
pub struct DelayLine {
    buffers: [Vec<f32>; 2],
    write_pos: usize,
    size: usize,
    wipe: Wipe,
}

impl DelayLine {
    /// Control thread (prepare): allocates; never called from `process`.
    pub fn allocate(&mut self, max_delay_samples: usize) {
        self.size = (max_delay_samples + 2).max(4);
        self.buffers = [vec![0.0; self.size], vec![0.0; self.size]];
        self.write_pos = 0;
        self.wipe.finish(self.size);
    }

    pub fn reset(&mut self) {
        for b in &mut self.buffers {
            b.fill(0.0);
        }
        self.write_pos = 0;
        self.wipe.finish(self.size);
    }

    /// Restart at the write head; stale slots read as zero and are wiped in slices.
    pub fn begin_clear(&mut self) {
        self.write_pos = 0;
        self.wipe.begin();
    }

    /// Clears up to `max_frames` of what `begin_clear` left; true once nothing is pending.
    pub fn clear_some(&mut self, max_frames: usize) -> bool {
        let (from, count) = self.wipe.claim(self.size, max_frames);
        for b in &mut self.buffers {
            b[from..from + count].fill(0.0);
        }
        self.wipe.done(self.size)
    }

    pub fn push(&mut self, left: f32, right: f32) {
        if self.size == 0 {
            return;
        }
        let i = self.write_pos;
        self.buffers[0][i] = left;
        self.buffers[1][i] = right;
        self.wipe.wrote(i);
        self.write_pos = if i + 1 == self.size { 0 } else { i + 1 };
    }

    /// `delay_samples` is clamped to `[1, capacity - 2]`; 1 = the most recent push.
    pub fn read(&self, channel: usize, delay_samples: f32) -> f32 {
        if self.size == 0 {
            return 0.0;
        }
        let d = delay_samples.clamp(1.0, (self.size - 2) as f32);
        let whole = d.floor() as usize;
        let frac = d - whole as f32;
        let b = &self.buffers[channel & 1];
        let at = |back: usize| {
            let slot = (self.write_pos + self.size - back) % self.size;
            if self.wipe.live(slot) {
                b[slot]
            } else {
                0.0
            }
        };
        at(whole) + frac * (at(whole + 1) - at(whole))
    }

    pub fn capacity(&self) -> usize {
        self.size
    }
}
