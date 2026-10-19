//! Deterministic test signals. Nothing here is random across runs: noise comes from a fixed-seed
//! LCG, so a failing test reproduces exactly.

use std::f64::consts::TAU;

/// Sine at `freq` Hz with peak amplitude 1.0, `frames` long.
pub fn sine(freq: f64, sample_rate: f64, frames: usize) -> Vec<f32> {
    sine_amp(freq, 1.0, sample_rate, frames)
}

/// Sine at `freq` Hz with peak amplitude `amp`. Computed in double, then narrowed.
pub fn sine_amp(freq: f64, amp: f32, sample_rate: f64, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|i| (f64::from(amp) * (TAU * freq * i as f64 / sample_rate).sin()) as f32)
        .collect()
}

/// Interleaved stereo sine with identical channels and peak amplitude `amp`.
pub fn stereo_sine(freq: f64, amp: f32, sample_rate: f64, frames: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    for s in sine_amp(freq, amp, sample_rate, frames) {
        out.push(s);
        out.push(s);
    }
    out
}

/// Uniform white noise in `[-1, 1)` from a 64-bit LCG (Knuth's MMIX constants).
pub fn noise(seed: u64, n: usize) -> Vec<f32> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // Top 24 bits give an exact f32 mantissa.
            let unit = (state >> 40) as f32 / 16_777_216.0;
            unit * 2.0 - 1.0
        })
        .collect()
}

/// A single impulse of height `amp` at frame 0, then zeros, `frames` long.
pub fn impulse(amp: f32, frames: usize) -> Vec<f32> {
    impulse_train(0, amp, frames)
}

/// Impulses of height `amp` every `period` frames, starting at frame 0. A `period` of 0 yields
/// silence.
pub fn impulse_train(period: usize, amp: f32, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames];
    if period > 0 {
        for v in out.iter_mut().step_by(period) {
            *v = amp;
        }
    }
    out
}
