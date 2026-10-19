//! Hotcue positions for one deck. Set and cleared from control threads, readable from any thread.

use std::sync::atomic::{AtomicI64, Ordering};

pub const NUM_CUES: usize = 8;
const EMPTY: i64 = -1;

#[derive(Debug)]
pub struct HotcueBank {
    cues: [AtomicI64; NUM_CUES],
}

impl Default for HotcueBank {
    fn default() -> Self {
        Self {
            cues: std::array::from_fn(|_| AtomicI64::new(EMPTY)),
        }
    }
}

impl HotcueBank {
    pub fn in_range(index: usize) -> bool {
        index < NUM_CUES
    }

    /// Stores `frame` (clamped to zero). Out-of-range indices are ignored.
    pub fn set(&self, index: usize, frame: i64) {
        if let Some(cue) = self.cues.get(index) {
            cue.store(frame.max(0), Ordering::Relaxed);
        }
    }

    pub fn clear(&self, index: usize) {
        if let Some(cue) = self.cues.get(index) {
            cue.store(EMPTY, Ordering::Relaxed);
        }
    }

    pub fn clear_all(&self) {
        for cue in &self.cues {
            cue.store(EMPTY, Ordering::Relaxed);
        }
    }

    pub fn get(&self, index: usize) -> Option<i64> {
        let value = self.cues.get(index)?.load(Ordering::Relaxed);
        (value != EMPTY).then_some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::HotcueBank;

    #[test]
    fn set_get_clear_and_range() {
        let bank = HotcueBank::default();
        assert_eq!(bank.get(0), None);
        bank.set(0, -5);
        assert_eq!(bank.get(0), Some(0), "negative frames clamp to zero");
        bank.set(3, 1234);
        assert_eq!(bank.get(3), Some(1234));
        bank.clear(3);
        assert_eq!(bank.get(3), None);
        bank.set(99, 1);
        assert_eq!(bank.get(99), None);
        bank.clear_all();
        assert_eq!(bank.get(0), None);
    }
}
