//! Atomic float cells: `f32`/`f64` values stored as their bit patterns, so a control thread can
//! publish a value and the audio thread can read it without a lock. Loads and stores are
//! `Relaxed`; callers that need ordering against other data must add their own fences.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const RELAXED: Ordering = Ordering::Relaxed;

/// `f32` cell shared between the control thread (writer) and the audio thread (reader).
#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }

    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(RELAXED))
    }

    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), RELAXED);
    }
}

/// `f64` cell shared between the control thread (writer) and the audio thread (reader).
#[derive(Debug, Default)]
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    pub fn new(v: f64) -> Self {
        Self(AtomicU64::new(v.to_bits()))
    }

    pub fn load(&self) -> f64 {
        f64::from_bits(self.0.load(RELAXED))
    }

    pub fn store(&self, v: f64) {
        self.0.store(v.to_bits(), RELAXED);
    }
}
