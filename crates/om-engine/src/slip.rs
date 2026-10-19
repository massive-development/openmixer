//! Slip mode bookkeeping: while a loop, scratch or hold runs, a ghost playhead keeps moving at the
//! underlying speed so playback can resume where the track would have been. Control thread only.
//! Port of `src/engine/SlipTracker.h`.

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct SlipTracker {
    ghost: f64,
    active: bool,
}

impl SlipTracker {
    pub fn begin(&mut self, source_frame: i64) {
        self.ghost = source_frame as f64;
        self.active = true;
    }

    /// Source frames the track would have played since the last call (elapsed time x rate).
    pub fn advance(&mut self, source_frames: f64) {
        if self.active {
            self.ghost += source_frames;
        }
    }

    /// Ends slip and returns the resume position clamped to the track, or `None` when slip was
    /// not active.
    pub fn end(&mut self, length_frames: i64) -> Option<i64> {
        if !self.active {
            return None;
        }
        self.active = false;
        let max = (length_frames - 1).max(0);
        Some((self.ghost as i64).clamp(0, max))
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumes_at_the_ghost_position() {
        let mut slip = SlipTracker::default();
        assert_eq!(slip.end(100_000), None);
        slip.begin(1000);
        slip.advance(500.0);
        slip.advance(500.0);
        assert!(slip.is_active());
        assert_eq!(slip.end(100_000), Some(2000));
        assert!(!slip.is_active());
        slip.begin(90_000);
        slip.advance(50_000.0);
        assert_eq!(slip.end(100_000), Some(99_999));
    }
}
