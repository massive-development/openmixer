//! Realtime helpers shared by the FX: parameter smoothing, denormal flushing and the atomic
//! float cells used to publish control-thread values to the audio thread.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const RELAXED: Ordering = Ordering::Relaxed;

/// Tiny feedback values become denormals on some CPUs; snapping them keeps the cost flat.
#[inline]
pub fn flush_denormal(x: f32) -> f32 {
    if x.abs() < 1.0e-15 {
        0.0
    } else {
        x
    }
}

/// One-pole smoother; a zero time constant makes it follow the target instantly.
#[derive(Debug, Clone, Copy)]
pub struct Smoother {
    coeff: f32,
    value: f32,
}

impl Default for Smoother {
    fn default() -> Self {
        Self {
            coeff: 1.0,
            value: 0.0,
        }
    }
}

impl Smoother {
    pub fn set_time(&mut self, sample_rate: f64, seconds: f64) {
        self.coeff = if seconds <= 0.0 {
            1.0
        } else {
            (1.0 - (-1.0 / (seconds * sample_rate)).exp()) as f32
        };
    }

    pub fn snap(&mut self, v: f32) {
        self.value = v;
    }

    pub fn current(&self) -> f32 {
        self.value
    }

    pub fn step(&mut self, target: f32) -> f32 {
        self.value += self.coeff * (target - self.value);
        if (target - self.value).abs() < 1.0e-6 {
            self.value = target;
        }
        self.value
    }
}

/// `f32` cell shared between the control thread (writer) and the audio thread (reader).
#[derive(Debug)]
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
#[derive(Debug)]
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
