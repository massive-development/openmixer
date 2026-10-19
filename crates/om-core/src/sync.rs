//! Poison-recovering lock access. A thread that panics while holding a `Mutex` poisons it; every
//! guarded value in this workspace stays structurally valid across such a panic, so callers take
//! the guard anyway rather than propagating the poison as a second panic.
//!
//! Not for audio-thread code: the audio path takes no locks at all.

use std::sync::{Mutex, MutexGuard};

/// Locks a `Mutex`, recovering the guard if a previous holder panicked.
pub trait LockExt<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|p| p.into_inner())
    }
}
