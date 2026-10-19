//! Loop bounds shared from control threads to the decoder and audio threads. Writers serialise on
//! a mutex (control side only); readers take a lock-free seqlock snapshot. Port of
//! `src/engine/LoopState.h`.

use std::sync::atomic::{fence, AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::Mutex;

/// Copy of the loop bounds. Valid when `end > start`; active when also enabled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoopSnapshot {
    pub start: i64,
    pub end: i64,
    pub enabled: bool,
}

impl LoopSnapshot {
    pub fn valid(&self) -> bool {
        self.end > self.start
    }

    pub fn active(&self) -> bool {
        self.enabled && self.valid()
    }
}

#[derive(Debug, Default)]
pub struct LoopState {
    write_lock: Mutex<()>,
    seq: AtomicU32,
    start: AtomicI64,
    end: AtomicI64,
    enabled: AtomicBool,
}

/// Attempts a reader makes before giving up on a snapshot while a writer is mid-update.
const READ_ATTEMPTS: usize = 4;

impl LoopState {
    pub fn store(&self, s: LoopSnapshot) {
        let _guard = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
        self.write_locked(s);
    }

    /// Read-modify-write under the writer lock: `change` sees the current bounds and returns the
    /// new ones, or `None` to leave them alone. Concurrent writers cannot slip in between the read
    /// and the write, so none of their updates is lost. Returns whether bounds were written.
    pub fn update(&self, change: impl FnOnce(&LoopSnapshot) -> Option<LoopSnapshot>) -> bool {
        let _guard = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
        let current = LoopSnapshot {
            start: self.start.load(Ordering::Relaxed),
            end: self.end.load(Ordering::Relaxed),
            enabled: self.enabled.load(Ordering::Relaxed),
        };
        match change(&current) {
            Some(next) => {
                self.write_locked(next);
                true
            }
            None => false,
        }
    }

    /// Caller holds `write_lock`.
    fn write_locked(&self, s: LoopSnapshot) {
        let v = self.seq.load(Ordering::Relaxed);
        self.seq.store(v.wrapping_add(1), Ordering::Relaxed);
        // Keeps the odd sequence number visible before any of the new fields.
        fence(Ordering::Release);
        self.start.store(s.start, Ordering::Relaxed);
        self.end.store(s.end, Ordering::Relaxed);
        self.enabled.store(s.enabled, Ordering::Relaxed);
        self.seq.store(v.wrapping_add(2), Ordering::Release);
    }

    /// Returns false only while a writer is mid-update; callers keep their previous snapshot.
    pub fn load(&self, out: &mut LoopSnapshot) -> bool {
        for _ in 0..READ_ATTEMPTS {
            let v = self.seq.load(Ordering::Acquire);
            if v & 1 != 0 {
                continue;
            }
            let snap = LoopSnapshot {
                start: self.start.load(Ordering::Relaxed),
                end: self.end.load(Ordering::Relaxed),
                enabled: self.enabled.load(Ordering::Relaxed),
            };
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == v {
                *out = snap;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_load_round_trip_and_validity() {
        let state = LoopState::default();
        let mut snap = LoopSnapshot::default();
        assert!(state.load(&mut snap));
        assert!(!snap.active());
        state.store(LoopSnapshot {
            start: 100,
            end: 200,
            enabled: true,
        });
        assert!(state.load(&mut snap));
        assert_eq!((snap.start, snap.end, snap.enabled), (100, 200, true));
        assert!(snap.active());
        state.store(LoopSnapshot {
            start: 300,
            end: 300,
            enabled: true,
        });
        assert!(state.load(&mut snap));
        assert!(!snap.valid() && !snap.active());
    }
}
