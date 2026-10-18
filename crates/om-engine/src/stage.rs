//! Insert chains: fixed slots of owned [`ProcessStage`]s that the audio thread runs in order.
//!
//! The audio side ([`StageChain`]) owns the stages. The control side ([`StageInsert`]) prepares a
//! stage on its own thread and queues it through a fixed-size command ring. The swap happens at the
//! start of the next `process` call. The displaced stage goes back on a second ring and is freed by
//! the control thread, so the audio thread never allocates, locks or drops.

use crate::fx::math::AtomicF64;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Interface version a stage must report to be inserted (C++ `ProcessStage::kInterfaceVersion`).
pub const INTERFACE_VERSION: u32 = 1;
/// Insert slots per chain.
pub const MAX_STAGES: usize = 4;
const RING_CAPACITY: usize = 32;

/// A stereo in-place audio stage that can be inserted into a channel or the master bus.
///
/// `prepare` runs on the control thread before insertion; `process` runs on the audio thread and
/// must not allocate, lock or perform I/O. `left` and `right` always have the same length.
pub trait ProcessStage: Send {
    /// Must equal [`INTERFACE_VERSION`] to be inserted.
    fn interface_version(&self) -> u32 {
        INTERFACE_VERSION
    }

    fn prepare(&mut self, _sample_rate: f64, _max_block_size: usize) {}

    fn process(&mut self, left: &mut [f32], right: &mut [f32]);
}

struct Command {
    slot: usize,
    stage: Option<Box<dyn ProcessStage>>,
}

/// Sample rate and block size the chain was last prepared with; read by the control side.
struct StageEnv {
    sample_rate: AtomicF64,
    max_block: AtomicUsize,
}

/// Audio-side owner of the slots. Built with [`StageChain::new`].
pub struct StageChain {
    slots: [Option<Box<dyn ProcessStage>>; MAX_STAGES],
    commands: rtrb::Consumer<Command>,
    retired: rtrb::Producer<Box<dyn ProcessStage>>,
    env: Arc<StageEnv>,
}

/// Control-side handle to a [`StageChain`]. Takes `&self`, so it can sit behind a shared handle.
pub struct StageInsert {
    commands: Mutex<rtrb::Producer<Command>>,
    retired: Mutex<rtrb::Consumer<Box<dyn ProcessStage>>>,
    env: Arc<StageEnv>,
}

impl StageChain {
    /// Empty chain plus its control-side handle. Control thread.
    pub fn new() -> (Self, StageInsert) {
        let env = Arc::new(StageEnv {
            sample_rate: AtomicF64::new(44_100.0),
            max_block: AtomicUsize::new(512),
        });
        let (command_tx, command_rx) = rtrb::RingBuffer::new(RING_CAPACITY);
        let (retired_tx, retired_rx) = rtrb::RingBuffer::new(RING_CAPACITY);
        let chain = Self {
            slots: std::array::from_fn(|_| None),
            commands: command_rx,
            retired: retired_tx,
            env: Arc::clone(&env),
        };
        let insert = StageInsert {
            commands: Mutex::new(command_tx),
            retired: Mutex::new(retired_rx),
            env,
        };
        (chain, insert)
    }

    /// Control thread, before the chain runs. Prepares every stage already in a slot.
    pub fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        self.env.sample_rate.store(sample_rate);
        self.env.max_block.store(max_block_size, Ordering::Relaxed);
        for stage in self.slots.iter_mut().flatten() {
            stage.prepare(sample_rate, max_block_size);
        }
    }

    /// Audio thread. Applies queued swaps, then runs every occupied slot in order.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.apply_commands();
        for stage in self.slots.iter_mut().flatten() {
            stage.process(left, right);
        }
    }

    fn apply_commands(&mut self) {
        while let Ok(Command { slot, stage }) = self.commands.pop() {
            if let Some(old) = std::mem::replace(&mut self.slots[slot], stage) {
                // Hand the old stage back instead of dropping it here. The control side reclaims
                // the ring on every `set`, so a full ring means it stopped reclaiming.
                let _ = self.retired.push(old);
            }
        }
    }
}

impl StageInsert {
    /// Inserts (`Some`) or clears (`None`) `slot`. Returns `false` for a bad slot, an incompatible
    /// interface version, or a full command ring. The stage is prepared before it is queued and
    /// takes effect at the next audio block. Control thread only.
    pub fn set(&self, slot: usize, mut stage: Option<Box<dyn ProcessStage>>) -> bool {
        if slot >= MAX_STAGES {
            return false;
        }
        if let Some(stage) = stage.as_mut() {
            if stage.interface_version() != INTERFACE_VERSION {
                return false;
            }
            let sample_rate = self.env.sample_rate.load();
            let max_block = self.env.max_block.load(Ordering::Relaxed);
            stage.prepare(sample_rate, max_block);
        }
        self.reclaim();
        lock(&self.commands).push(Command { slot, stage }).is_ok()
    }

    pub fn clear(&self, slot: usize) -> bool {
        self.set(slot, None)
    }

    /// Frees stages the audio thread has displaced. Control thread only.
    pub fn reclaim(&self) {
        let mut retired = lock(&self.retired);
        while retired.pop().is_ok() {}
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scale(f32);

    impl ProcessStage for Scale {
        fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                *l *= self.0;
                *r *= self.0;
            }
        }
    }

    #[test]
    fn swap_takes_effect_on_the_next_block_and_clear_restores_bypass() {
        let (mut chain, insert) = StageChain::new();
        let mut l = [1.0_f32; 4];
        let mut r = [1.0_f32; 4];
        assert!(insert.set(0, Some(Box::new(Scale(0.5)))));
        chain.process(&mut l, &mut r);
        assert_eq!(l, [0.5; 4]);
        assert!(insert.set(0, Some(Box::new(Scale(0.25)))));
        l = [1.0; 4];
        chain.process(&mut l, &mut r);
        assert_eq!(l, [0.25; 4]);
        assert!(insert.clear(0));
        l = [1.0; 4];
        chain.process(&mut l, &mut r);
        assert_eq!(l, [1.0; 4]);
        insert.reclaim();
    }

    #[test]
    fn rejects_bad_slots_and_foreign_interface_versions() {
        struct Stale;
        impl ProcessStage for Stale {
            fn interface_version(&self) -> u32 {
                999
            }
            fn process(&mut self, _: &mut [f32], _: &mut [f32]) {}
        }
        let (_chain, insert) = StageChain::new();
        assert!(!insert.set(MAX_STAGES, Some(Box::new(Scale(1.0)))));
        assert!(!insert.set(0, Some(Box::new(Stale))));
    }
}
