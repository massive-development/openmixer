//! Sliced buffer wipe for ring buffers: slots below `valid` hold real (or zero) data, the rest are
//! stale and must read as zero. Lets an effect be engaged while its buffer is still being wiped, so
//! no audio callback clears more than (blockSize / 64) * SLICE frames per unit. Audio thread only.

/// Max frames wiped per 64-sample chunk (per effect, all buffers).
pub const SLICE: usize = 512;

#[derive(Debug, Default, Clone, Copy)]
pub struct Wipe {
    valid: usize,
}

impl Wipe {
    /// Buffer is fully clean.
    pub fn finish(&mut self, size: usize) {
        self.valid = size;
    }

    /// Everything is stale.
    pub fn begin(&mut self) {
        self.valid = 0;
    }

    pub fn live(&self, slot: usize) -> bool {
        slot < self.valid
    }

    /// Writes advance the frontier.
    pub fn wrote(&mut self, slot: usize) {
        if slot == self.valid {
            self.valid += 1;
        }
    }

    pub fn done(&self, size: usize) -> bool {
        self.valid >= size
    }

    /// Claims up to `max_frames` stale slots; returns `(from, count)` and the caller zeroes that range.
    pub fn claim(&mut self, size: usize, max_frames: usize) -> (usize, usize) {
        let from = self.valid;
        let count = size.saturating_sub(self.valid).min(max_frames);
        self.valid += count;
        (from, count)
    }
}
